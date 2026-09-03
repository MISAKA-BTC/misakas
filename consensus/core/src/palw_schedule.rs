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

/// The degraded-ladder round budget the challenge window was intended to fit (ADR-0028 §3,
/// ADR-0027 §1: ≈ log₂ of the step count at the credited ceiling).
///
/// **An aspiration, not a fact about any shipped preset.** A rung costs
/// [`PALW_SCHEDULE_WINDOWS_PER_RUNG`] windows and the terminal opening plus the conviction cost
/// [`PALW_SCHEDULE_WINDOWS_AFTER_LADDER`] more, so 20 rounds needs `w_replay + 42 · w_round` — 15 480
/// DAA at the deci-bps defaults against a `w_challenge` of 8 640, and the pruning horizon caps that
/// preset at 12 rounds no matter how `w_challenge` is raised. The 120 s preset affords 10 and caps at
/// 17. What a parameter set can actually walk is [`affordable_ladder_rounds_v1`]; the space it can
/// adjudicate is [`max_ladder_space_v1`]. Keep this constant as the target the ADR set, and read the
/// derived pair for what is true.
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
    #[error(
        "this class's worst-case step space is {leaves} leaves and the ladder can reach {reachable} — \
         a fraud deeper than that is unprosecutable inside the challenge window"
    )]
    LadderCannotReachTheClass { leaves: u64, reachable: u64 },
    #[error("block cadence is frozen at {required_ms} ms per block (ADR-0038 Decision H); this network targets {got_ms} ms")]
    CadenceNotFrozen { got_ms: u64, required_ms: u64 },
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
///
/// # A duplicated `validator_id` removes that id from the draw, fail-closed
///
/// `validator_id` is a `validator_pubkey_hash`, which `dns_finality` states is NOT unique: two
/// bonds may share one key. Two such candidates produce IDENTICAL ticket tuples here — same
/// preimage, same tie-break — so both survived `truncate(q)` and the panel came back as
/// `[X, X]`. That was mintable, not cosmetic: the credit walk matched one attestation against
/// both seats and paid the same bond twice for one signature, inside the per-block ceiling. It
/// also left the panel holding ONE real verifier, so with `q = 2` a single colluding partner
/// satisfied the whole "≥ 1 assigned attestation" predicate alone, and the other bonded
/// validator was undrawable forever.
///
/// Dropping the id entirely is deliberate and is the same construction
/// `palw_routing::select_routed_replay_panel_v1` already uses. Picking one of two
/// indistinguishable records would make the panel depend on candidate order — this function
/// promises order-invariance — and a duplicate here means the CALLER failed to key its candidate
/// set on a unique identity, which is a bug to surface rather than to average over. The
/// seat-based `palw_job_panel::select_job_panel_v3` is the real answer, because it keys on the
/// bond outpoint and can seat both bonds honestly.
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
    let mut occurrences: HashMap<Hash64, u32> = HashMap::new();
    for c in candidates {
        *occurrences.entry(c.validator_id).or_insert(0) += 1;
    }
    let mut ticketed: Vec<(Hash64, Hash64)> = candidates
        .iter()
        .filter(|c| occurrences[&c.validator_id] == 1)
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

/// Windows one bisection RUNG actually consumes: two, not one.
///
/// `apply_disclosure` sets the deadline for the challenger's verdict, and `apply_verdict` then sets
/// the deadline for the next disclosure — so one rung of the ladder burns `2 · w_round`. The
/// original budget charged one window per round and was therefore short by a factor of two, plus the
/// terminal opening and the conviction that follow the last rung. The cost of being short is not a
/// slow ladder: a step conviction counts only if it is accepted at or before
/// `commitment_accepted_daa + w_challenge`, so a ladder that overruns produces a conviction that is
/// DISCARDED and an executor that is never slashed.
pub const PALW_SCHEDULE_WINDOWS_PER_RUNG: u64 = 2;

/// Windows the ladder needs AFTER its last rung: one for the terminal opening the responder owes,
/// one for the conviction it enables. Both must land inside `w_challenge` or they are telemetry.
pub const PALW_SCHEDULE_WINDOWS_AFTER_LADDER: u64 = 2;

/// How many bisection rounds a parameter set can actually afford inside its own challenge window.
///
/// `(w_challenge − w_replay − after · w_round) / (per_rung · w_round)`, floored at zero. This is the
/// number [`PALW_SCHEDULE_LADDER_ROUNDS`] aspires to and **no shipped preset reaches**: measured on
/// the two Stage-1 defaults, deci-bps affords 10 rounds and the 120 s preset affords 10, against the
/// 20 the constant names. Raising `w_challenge` does not rescue it either — the pruning horizon caps
/// deci-bps at 12 rounds and the 120 s preset at 17.
///
/// The consequence is a real limit on what can be adjudicated, and it is better stated than
/// discovered: a step space larger than `2^affordable` cannot be bisected to a terminal index before
/// the challenge window closes, so a fraud that deep is unprosecutable at these parameters. See
/// [`max_ladder_space_v1`].
pub fn affordable_ladder_rounds_v1(params: &PalwScheduleParamsV1) -> u64 {
    if params.w_round == 0 {
        return 0;
    }
    let after = PALW_SCHEDULE_WINDOWS_AFTER_LADDER.saturating_mul(params.w_round);
    let usable = params.w_challenge.saturating_sub(params.w_replay).saturating_sub(after);
    usable / (PALW_SCHEDULE_WINDOWS_PER_RUNG * params.w_round)
}

/// **Can this class's disputes actually be adjudicated under these windows?** (audit P0-9 item 4)
///
/// The audit's own remedy: *measure the real `step_leaf_count` and make "a terminal verdict is
/// reachable inside the challenge window, for every class" an activation condition*. This is that
/// condition, as a refusal rather than a note.
///
/// A step space larger than [`max_ladder_space_v1`] cannot be bisected to a terminal index before
/// `w_challenge` closes, so the terminal opening and the conviction it enables land past the window
/// and are discarded. A fraud that deep is **unprosecutable** — not slow, not expensive:
/// structurally beyond reach — and a network that activates such a class has a court that cannot
/// convict, which is the assumption A4 leans on.
///
/// The worst case is what is checked, at the profile's own `n_ctx`: a class is admitted or not, and
/// admitting one whose TYPICAL job fits while its longest does not is admitting a class an attacker
/// picks the job length for.
///
/// `Err` names both numbers, because "your ladder is too short" and "your model is too big" are the
/// same fact seen from two ends and the operator has to see which end they can move.
pub fn class_is_adjudicable_v1(
    profile: &crate::palw_step::PalwShapeProfileV3,
    params: &PalwScheduleParamsV1,
) -> Result<u64, PalwScheduleError> {
    // **The executor's constant, and it is the DEFAULT rather than the rule** — for a caller with
    // no ruleset in hand. `Params::validate_palw_v2` HAS one (`palw_consensus_mode`'s
    // `court.max_step_leaf_count()`) and should call [`class_is_adjudicable_capped_v1`] with it;
    // see this fixer's patch note. Every shipped preset leaves `palw_schedule` at `None`, so the
    // gate below is unreachable on all four and the difference is latent rather than live.
    class_is_adjudicable_capped_v1(profile, params, crate::palw_step::PALW_STEP_MAX_LEAVES)
}

/// [`class_is_adjudicable_v1`] against the ladder the RULESET froze (ADR-0082 Decision 1: the
/// ruleset's ladder is read from the bundle, never typed).
///
/// The two refusals are different facts and only one of them is this gate's: a class whose worst
/// case is deeper than the LADDER is not a schedule problem at all (admission refuses it by name,
/// `DeeperThanTheLadder`), while a class the ladder admits and these WINDOWS cannot walk is
/// exactly `LadderCannotReachTheClass`. Counting against the executor's `2^22` collapsed the two:
/// on a `2^26` ruleset the honest answer "your windows afford ten rounds, the class needs
/// twenty-three" arrived as `ParamsNotCanonical`, naming neither number the operator can move.
pub fn class_is_adjudicable_capped_v1(
    profile: &crate::palw_step::PalwShapeProfileV3,
    params: &PalwScheduleParamsV1,
    max_step_leaf_count: u64,
) -> Result<u64, PalwScheduleError> {
    // The longest job this class admits. `exact_decode_tokens` of 1 with the whole context as
    // prefill is the largest leaf count the enumeration can reach for a given `n_ctx`.
    // `step_leaf_count` reads only the token counts off the context, so the worst case is computed
    // from those alone rather than by inventing a whole job — a synthetic job id or seed here would
    // be a value nobody checks that a reader could mistake for one that matters.
    let leaves = crate::palw_step::worst_case_step_leaf_count_capped_v1(profile, max_step_leaf_count)
        .map_err(|_| PalwScheduleError::ParamsNotCanonical { reason: "the class's own shape exceeds the step-space cap" })?;
    let reachable = max_ladder_space_v1(params);
    if leaves > reachable {
        return Err(PalwScheduleError::LadderCannotReachTheClass { leaves, reachable });
    }
    Ok(leaves)
}

/// The largest bisection space these parameters can walk to a terminal index in time.
///
/// `2^affordable_rounds`, capped at [`crate::palw_bisect::PALW_BISECT_MAX_SPACE`]. A ladder opened
/// over a larger space is not merely slow — its terminal opening and conviction land past
/// `w_challenge` and are discarded, so the ladder cannot convict anyone and the honest challenger
/// spends its rungs for nothing. `PALW_STEP_MAX_LEAVES` is `2^22` and the global bisect cap is
/// `2^40`; neither is reachable at any parameters a shipped preset can carry, which is the fact this
/// function exists to make visible rather than to hide.
pub fn max_ladder_space_v1(params: &PalwScheduleParamsV1) -> u64 {
    let rounds = affordable_ladder_rounds_v1(params);
    if rounds >= 40 { crate::palw_bisect::PALW_BISECT_MAX_SPACE } else { 1u64 << rounds }
}

/// ADR-0038 Decision H: the frozen PALW block interval, in milliseconds.
///
/// One block per 120 seconds on every network carrying value. See
/// [`PalwScheduleParamsV1::validate_for_value_network_v1`] for the two measurements that fix it.
pub const PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS: u64 = 120_000;

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
    /// ADR-0038 Decision H: these params are admissible on a network CARRYING VALUE.
    ///
    /// [`Self::validate`] answers whether the windows are internally consistent. This answers a
    /// second question that no relationship among the windows can express, because it is about the
    /// cadence they sit on: **a PALW network targets one block per 120 seconds, frozen.**
    ///
    /// Two independent measurements force it, either alone sufficient.
    ///
    /// * **Sync headroom** = block interval ÷ per-header verification cost. Below 1× a joining node
    ///   falls further behind with every header it verifies and can never finish — the network is
    ///   permanently closed to new participants, which is not a slow sync but a dead one. Measured
    ///   on the reference x86-64 CPU class against the pinned Qwen3.5-2B: 15.7 s per header clean,
    ///   so 120 s gives 3.0–5.4× and the 10-second preset gives **0.64×**. The faster preset is not
    ///   aggressive; it is outside the feasible set for this class.
    /// * **Ladder depth.** The pruning horizon caps [`affordable_ladder_rounds_v1`] at 12 rounds on
    ///   the deci-bps preset and 17 on this one — a 32× difference in the step space that can be
    ///   walked to a terminal index before `w_challenge` closes. A faster cadence forecloses the
    ///   court permanently, and no implementation work reopens it.
    ///
    /// Kept OUT of `validate` on purpose: the deci-bps preset is internally consistent and stays
    /// valid for tests. What it is not is admissible, and conflating "well-formed" with "may carry
    /// value" is how a test preset reaches a network.
    pub fn validate_for_value_network_v1(&self, blockrate: &BlockrateParams) -> Result<(), PalwScheduleError> {
        // The cadence is checked FIRST, and the order is the point. Run the window arithmetic
        // first and a caller who shortened the interval gets a window-inequality error about
        // pruning depth — true, but it reads as "widen a window", which is the repair that cannot
        // work. The frozen fact should be the message.
        if blockrate.target_time_per_block != PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS {
            return Err(PalwScheduleError::CadenceNotFrozen {
                got_ms: blockrate.target_time_per_block,
                required_ms: PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS,
            });
        }
        self.validate(blockrate)
    }

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
        // The ladder must be able to reach a VERDICT inside the challenge window, not merely to
        // start. The original form charged one window per round; a rung burns two (the disclosure
        // sets the verdict's deadline, the verdict sets the next disclosure's), and the terminal
        // opening plus the conviction need one each after the last rung. Being short here does not
        // make a ladder slow — a conviction accepted past `w_challenge` is discarded, so the
        // executor is never slashed.
        //
        // The check is on ONE affordable rung plus the tail rather than on
        // `PALW_SCHEDULE_LADDER_ROUNDS`, because no shipped preset can afford 20 at any
        // `w_challenge` its pruning horizon permits (measured: deci-bps 12, the 120 s preset 17).
        // Demanding 20 here would reject every preset in the tree; what the network can actually
        // adjudicate is exposed by `affordable_ladder_rounds_v1` and bounded by
        // `max_ladder_space_v1` instead of asserted.
        let minimum = self
            .w_replay
            .checked_add(
                PALW_SCHEDULE_WINDOWS_PER_RUNG
                    .checked_add(PALW_SCHEDULE_WINDOWS_AFTER_LADDER)
                    .and_then(|w| w.checked_mul(self.w_round))
                    .ok_or(PalwScheduleError::DaaOverflow { what: "ladder budget" })?,
            )
            .ok_or(PalwScheduleError::DaaOverflow { what: "ladder budget" })?;
        if self.w_challenge < minimum {
            return Err(PalwScheduleError::WindowInequalityViolated {
                rule: "w_challenge ≥ w_replay + (WINDOWS_PER_RUNG + WINDOWS_AFTER_LADDER) · w_round",
            });
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

    /// **H13, measured rather than asserted: the shipped mainnet identity cannot carry PALW.**
    ///
    /// ADR-0036 Decision 2 and the readiness audit reached this independently, and both state it
    /// as a conclusion. A conclusion in prose is a thing that can quietly stop being true — a
    /// preset edit, a Crescendo retune — so it is pinned here as arithmetic over the values the
    /// binary actually ships.
    ///
    /// Two independent refusals, and it matters that they are independent: fixing either one
    /// alone leaves the other standing, which is why the answer is a new network identity rather
    /// than a parameter change.
    #[test]
    fn the_shipped_mainnet_identity_cannot_carry_a_palw_schedule() {
        use crate::config::params::MAINNET_PARAMS;

        let blockrate = &MAINNET_PARAMS.blockrate;
        // 1. The cadence. PALW is frozen at 120 s/block (ADR-0038 Decision H); mainnet runs
        //    10 BPS. This is not a window that can be widened — every DAA-denominated window in
        //    the ruleset takes its wall-clock meaning from the cadence, so a PALW ruleset on a
        //    100 ms chain means something different by every number in it.
        assert_ne!(blockrate.target_time_per_block, PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS);
        assert_eq!(blockrate.target_time_per_block, 100, "mainnet is 10 BPS — 100 ms per block");

        // 2. The window inequality, which fails for BOTH shipped presets and fails on the
        //    finality depth rather than on anything a schedule can choose. At 10 BPS the depth is
        //    orders of magnitude above either challenge window, so no preset in this file fits.
        for (name, params) in [
            ("deci-bps", PalwScheduleParamsV1::stage1_defaults_deci_bps()),
            ("two-minute", PalwScheduleParamsV1::stage1_defaults_two_minute_bps()),
        ] {
            assert!(
                blockrate.finality_depth >= params.w_challenge,
                "{name}: finality_depth {} must be >= w_challenge {} for this to be the refusal under test",
                blockrate.finality_depth,
                params.w_challenge
            );
            assert!(
                matches!(params.validate_for_value_network_v1(blockrate), Err(PalwScheduleError::CadenceNotFrozen { .. })),
                "{name}: a value network must refuse on the cadence first — the frozen fact is the message"
            );
            // …and the window rule refuses it too, independently of the cadence: `validate`
            // skips the cadence clause, so this is the second refusal standing on its own.
            assert!(
                matches!(
                    params.validate(blockrate),
                    Err(PalwScheduleError::WindowInequalityViolated { rule: "finality_depth < w_challenge" })
                ),
                "{name}: the window inequality must fail on its own, not only behind the cadence"
            );
        }

        // The same schedule on a 120 s chain with a proportionate finality depth is admissible —
        // so what this test measures is the mainnet IDENTITY, not the schedule presets.
        let mut palw_rate = blockrate.clone();
        palw_rate.target_time_per_block = PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS;
        palw_rate.finality_depth = 600;
        PalwScheduleParamsV1::stage1_defaults_two_minute_bps()
            .validate_for_value_network_v1(&palw_rate)
            .expect("the two-minute preset fits a two-minute network");
    }
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

    /// A `validator_pubkey_hash` shared by two bonds used to seat BOTH — identical tickets,
    /// identical tie-break, so `truncate(q)` kept them both — and the credit walk then paid one
    /// signature twice. `dns_finality` permits that key sharing, so this was reachable without
    /// any protocol violation.
    #[test]
    fn two_bonds_under_one_validator_key_cannot_seat_the_same_id_twice() {
        let mut cands = candidates();
        // NOT `cands[1]`: its id is h64(2), which is the executor in these vectors, so it is
        // excluded for a different reason and the duplicate would never be the thing under test.
        let twin = cands[3].validator_id;
        assert_ne!(twin, h64(0x02), "the twin must not be the executor");
        cands.push(PalwPanelCandidateV1 { validator_id: twin, ..cands[3] });
        let panel = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 8);
        // The panel contains no id twice — the property the mint arithmetic relies on.
        let mut unique = panel.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), panel.len(), "a duplicated id must never seat twice: {panel:?}");
        // And fail-closed: the ambiguous id is out of the draw entirely, rather than seated once
        // by whichever record the caller happened to list first.
        assert!(!panel.contains(&twin), "an ambiguous identity is not silently resolved");
        // Every unambiguous candidate is unaffected — the refusal is scoped to the duplicate.
        // (h64(0x02) is the executor in these vectors and is out for its own reason.)
        for c in candidates().iter().filter(|c| c.validator_id != twin && c.validator_id != h64(0x02)) {
            assert!(panel.contains(&c.validator_id), "an unambiguous candidate lost its seat: {:?}", c.validator_id);
        }
        assert_eq!(panel.len(), 6, "8 candidates minus the executor minus the ambiguous pair");
        // Order-invariant, which is why dropping beats picking: listing the twin first must not
        // change anything.
        let mut reversed = cands.clone();
        reversed.reverse();
        assert_eq!(select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &reversed, 8), panel);
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
        all.extend_from_slice(crate::palw_job_panel::PALW_PANEL_ALL_DOMAINS);
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

    /// **The ladder cannot reach the pinned model's step space, at any shipped preset.**
    ///
    /// The affordable space is `2^10 = 1 024` on both presets, and the pruning horizon caps it at
    /// `2^12` (deci-bps) / `2^17` (120 s). This test asks what the pinned Qwen3.5-2B actually needs,
    /// and the answer clears the affordable space by four orders of magnitude at the coarsest
    /// tiling anyone would use.
    ///
    /// **The envelope, not a point estimate**, because two inputs are not pinned anywhere in this
    /// tree and one of them cannot be: the per-layer node count and the tile length live in a
    /// `PalwShapeProfileV3` that has never been built for this model — every profile in the
    /// repository is a test fixture, `fleet_registration()`'s included, at `layer_count: 4,
    /// hidden_dim: 16`. So the test sweeps the plausible range and asserts the CONCLUSION, which is
    /// the same everywhere in it. A point estimate here would be a number I cannot source.
    ///
    /// Geometry that IS pinned: `MODEL_LAYER_COUNT = 24`, `MODEL_HIDDEN_DIM = 2048` (`vlt.rs`).
    ///
    /// `leaves_per_position = Σ_nodes ceil(out_len / tile_len)` over pre ‖ 24 layers ‖ post, and
    /// the job's positions multiply it. A 14-second CPU inference is not a handful of tokens.
    #[test]
    fn the_pinned_model_needs_a_deeper_ladder_than_any_preset_affords() {
        const LAYERS: u64 = 24; // vlt::qwen35_pins::MODEL_LAYER_COUNT
        const HIDDEN: u64 = 2_048; // vlt::qwen35_pins::MODEL_HIDDEN_DIM

        let deci = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        let two_minute = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        assert_eq!(max_ladder_space_v1(&deci), 1 << 10);
        assert_eq!(max_ladder_space_v1(&two_minute), 1 << 10);

        // The whole plausible envelope: a transformer layer is at least a norm, a projection and an
        // output (3) and at most a fully split attention+FFN graph (12); tiles run from the whole
        // hidden row down to a 256-element strip; a 14 s inference is at least 16 tokens.
        for nodes_per_layer in [3u64, 6, 12] {
            for tile in [HIDDEN, 512, 256] {
                for positions in [16u64, 64, 256] {
                    let per_position = LAYERS * nodes_per_layer * HIDDEN.div_ceil(tile);
                    let leaves = per_position * positions;
                    assert!(
                        leaves > max_ladder_space_v1(&two_minute),
                        "nodes/layer {nodes_per_layer}, tile {tile}, {positions} positions -> {leaves} leaves, \
                         which the ladder would have to bisect in {} rounds",
                        (leaves as f64).log2().ceil()
                    );
                }
            }
        }

        // The floor of that envelope — 3 nodes, whole-row tiles, 16 tokens — already needs 11
        // rounds against the 10 both presets afford. The pruning ceiling (12 / 17) is what a
        // proposal to close this has to work inside, and only the 120 s preset has room at all,
        // which is the second reason Decision H froze the cadence there.
        let floor_leaves = (LAYERS * 3) * 16;
        assert_eq!(floor_leaves, 1_152);
        assert!(floor_leaves > max_ladder_space_v1(&two_minute), "even the floor of the envelope exceeds the affordable space");
    }

    /// **Audit P0-9 item 4**: a class whose disputes cannot terminate in time is refused.
    ///
    /// The audit's own remedy — measure the real step space and make reachability an ACTIVATION
    /// CONDITION — as a refusal. A class larger than the ladder can walk has a court that cannot
    /// convict at any depth beyond it, which is A4's assumption failing silently rather than a
    /// network being slow.
    ///
    /// The toy fixture passes and a realistic geometry does not, which is the whole point: the
    /// check is what turns "nobody measured this" into "this network cannot be built".
    #[test]
    fn a_class_the_ladder_cannot_walk_is_refused() {
        use crate::palw_step::PalwShapeProfileV3;
        let two_minute = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();

        // A tiny fixture class fits inside 2^10 and is admitted, with its measured space returned.
        let small = crate::palw_registry::tests::profile_for_schedule_probe();
        let leaves = class_is_adjudicable_v1(&small, &two_minute).expect("a toy class fits the ladder");
        assert!(leaves <= max_ladder_space_v1(&two_minute));

        // Widen the same shape toward the pinned model's depth and it stops being adjudicable.
        let mut realistic = PalwShapeProfileV3 { layer_count: 24, n_ctx: 64, ..small };
        realistic.n_batch = realistic.n_ctx;
        realistic.n_ubatch = realistic.n_ctx;
        let err = class_is_adjudicable_v1(&realistic, &two_minute).expect_err("24 layers over 64 tokens outruns 2^10");
        assert!(
            matches!(err, PalwScheduleError::LadderCannotReachTheClass { reachable, .. } if reachable == max_ladder_space_v1(&two_minute)),
            "the error must name both ends, since raising the ladder and shrinking the class are the same fact: {err}"
        );
    }

    /// ADR-0038 Decision H: the 120-second cadence is frozen, and "well-formed" does not imply
    /// "may carry value".
    ///
    /// The deci-bps preset is internally consistent — `validate` accepts it — and is still refused
    /// for a value network, because the two facts that rule it out are not relationships among its
    /// own windows: its sync headroom against the pinned model is 0.64× (below 1×, so no node can
    /// ever finish a join), and the pruning horizon caps its ladder at 2^12 against this preset's
    /// 2^17. Conflating the two questions is how a test preset reaches a network.
    #[test]
    fn only_the_120_second_cadence_may_carry_value() {
        let two_minute = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        two_minute
            .validate_for_value_network_v1(&BlockrateParams::new_two_minute_bps())
            .expect("the frozen cadence is the admissible one");

        let deci = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        deci.validate(&BlockrateParams::new_deci_bps()).expect("deci-bps is WELL-FORMED");
        assert!(
            matches!(
                deci.validate_for_value_network_v1(&BlockrateParams::new_deci_bps()),
                Err(PalwScheduleError::CadenceNotFrozen { required_ms: PALW_FROZEN_TARGET_TIME_PER_BLOCK_MS, .. })
            ),
            "a well-formed test preset must still be refused for a value network"
        );

        // And the check is on the CADENCE, not on which preset was passed: keeping the admissible
        // windows and shortening the interval is refused, and refused with THIS error rather than
        // the window-inequality one those windows would also trip on a 10 s chain. The order in
        // `validate_for_value_network_v1` is what decides that, so it is asserted, not assumed —
        // a pruning-depth complaint reads as "widen a window", the one repair that cannot work.
        assert!(matches!(
            two_minute.validate_for_value_network_v1(&BlockrateParams::new_deci_bps()),
            Err(PalwScheduleError::CadenceNotFrozen { got_ms: 10_000, .. })
        ));
        assert!(
            matches!(two_minute.validate(&BlockrateParams::new_deci_bps()), Err(PalwScheduleError::WindowInequalityViolated { .. })),
            "the window error this shadows is real — which is exactly why the cadence check runs first"
        );
    }

    /// **No shipped preset can afford the ladder ADR-0028 §3 budgets, and the shortfall is a
    /// silently-discarded conviction rather than a slow dispute.**
    ///
    /// A rung costs two windows (the disclosure sets the verdict's deadline, the verdict sets the
    /// next disclosure's), and the terminal opening plus the conviction cost one each after the last
    /// rung. The old inequality charged ONE window per round, so it certified a 20-round ladder that
    /// needs `w_replay + 42 · w_round` against a budget of `w_replay + 20 · w_round`.
    ///
    /// Every number below is measured, and each is pinned so a parameter edit that quietly shrinks
    /// what the network can adjudicate fails here.
    #[test]
    fn no_shipped_preset_affords_the_twenty_round_ladder_the_adr_budgets() {
        for (name, params, blockrate, affordable, ceiling_rounds) in [
            ("deci-bps", PalwScheduleParamsV1::stage1_defaults_deci_bps(), BlockrateParams::new_deci_bps(), 10u64, 12u64),
            ("two-minute", PalwScheduleParamsV1::stage1_defaults_two_minute_bps(), BlockrateParams::new_two_minute_bps(), 10, 17),
        ] {
            // The preset is valid — the corrected inequality demands one rung plus the tail, which
            // both presets clear.
            params.validate(&blockrate).unwrap();

            // But it affords far fewer rounds than the ADR's 20.
            assert_eq!(affordable_ladder_rounds_v1(&params), affordable, "{name} affordable rounds moved");
            assert!(affordable < PALW_SCHEDULE_LADDER_ROUNDS, "{name} must not be read as affording the aspiration");
            assert_eq!(max_ladder_space_v1(&params), 1u64 << affordable, "{name} adjudicable space moved");

            // The 20-round cost, stated: what `w_challenge` would have to be.
            let needed = params.w_replay
                + (PALW_SCHEDULE_WINDOWS_PER_RUNG * PALW_SCHEDULE_LADDER_ROUNDS + PALW_SCHEDULE_WINDOWS_AFTER_LADDER) * params.w_round;
            assert!(needed > params.w_challenge, "{name}: 20 rounds would need {needed} > {}", params.w_challenge);

            // And raising `w_challenge` does NOT rescue it: the pruning horizon is the hard cap, and
            // even at the largest admissible window the affordable rounds stay below 20.
            let max_challenge = blockrate.pruning_depth - params.prosecution_slack - 1;
            let stretched = PalwScheduleParamsV1 { w_challenge: max_challenge, ..params };
            assert_eq!(affordable_ladder_rounds_v1(&stretched), ceiling_rounds, "{name} pruning-capped rounds moved");
            assert!(ceiling_rounds < PALW_SCHEDULE_LADDER_ROUNDS, "{name}: the horizon itself forbids the aspiration");
            // One DAA further and the pruning inequality refuses it, so this really is the ceiling.
            let over = PalwScheduleParamsV1 { w_challenge: max_challenge + 1, ..params };
            assert!(over.validate(&blockrate).is_err(), "{name}: the horizon must bind");
        }

        // The step-leg cap and the global bisect cap are both far past anything reachable, which is
        // the fact these accessors exist to surface rather than to hide.
        let deci = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        assert!(max_ladder_space_v1(&deci) < crate::palw_step::PALW_STEP_MAX_LEAVES);
        assert!(max_ladder_space_v1(&deci) < crate::palw_bisect::PALW_BISECT_MAX_SPACE);
    }

    /// The derived pair is monotone and degenerate-safe: a zero round window affords nothing rather
    /// than dividing by zero, and a window that cannot even fit the tail affords zero rounds.
    #[test]
    fn the_affordable_ladder_is_monotone_and_never_divides_by_zero() {
        let base = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        assert_eq!(affordable_ladder_rounds_v1(&PalwScheduleParamsV1 { w_round: 0, ..base }), 0);
        assert_eq!(max_ladder_space_v1(&PalwScheduleParamsV1 { w_round: 0, ..base }), 1, "no rung means no bisection");
        // Below the tail's own cost: zero rounds, not a wrapped huge number.
        assert_eq!(affordable_ladder_rounds_v1(&PalwScheduleParamsV1 { w_challenge: base.w_replay, ..base }), 0);
        // Monotone in the window and inversely monotone in the rung cost.
        let wider = PalwScheduleParamsV1 { w_challenge: base.w_challenge * 2, ..base };
        assert!(affordable_ladder_rounds_v1(&wider) > affordable_ladder_rounds_v1(&base));
        let slower = PalwScheduleParamsV1 { w_round: base.w_round * 2, ..base };
        assert!(affordable_ladder_rounds_v1(&slower) < affordable_ladder_rounds_v1(&base));
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

        // One rung plus the terminal opening plus the conviction — four windows — must fit. At
        // w_round = 2 100 that is 360 + 8 400 = 8 760 against a w_challenge of 8 640.
        let mut ladder_broken = good;
        ladder_broken.w_round = 2_100;
        assert_eq!(
            ladder_broken.validate(&blockrate),
            Err(PalwScheduleError::WindowInequalityViolated {
                rule: "w_challenge ≥ w_replay + (WINDOWS_PER_RUNG + WINDOWS_AFTER_LADDER) · w_round"
            })
        );
        // And the boundary: 2 070 fits exactly (360 + 8 280 = 8 640), affording one rung and no more.
        let mut exactly_one_rung = good;
        exactly_one_rung.w_round = 2_070;
        exactly_one_rung.validate(&blockrate).unwrap();
        assert_eq!(affordable_ladder_rounds_v1(&exactly_one_rung), 1);
        assert_eq!(max_ladder_space_v1(&exactly_one_rung), 2, "one rung bisects a space of two");

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
