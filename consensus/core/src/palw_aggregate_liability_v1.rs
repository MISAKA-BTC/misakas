//! **ADR-0160 lane liab — aggregate bond liability** (testnet-12 only, post-launch, behind the
//! dormant `Params::palw_capacity_aggregate_liability`, F-L).
//!
//! The user's design (2026-09-26), point 3: "per-claim担保からaggregate bond liabilityへ: bond=13,000
//! MSK、claim#1..#100 → shared slash pool。条件: licence前はrewardを引き出せない / 不正証明が出たら
//! bond全体をaggregate forfeiture / voidしても義務は消えない / 同じbondによる連続不正は最初の検出で
//! 全claimをfreeze". This module holds the pure half of it; the fold half is
//! `TransitionBuilder::aggregate_on_conviction_v1` in `palw_state_v2.rs`, the one funnel every
//! conviction's record (`close_conviction_v1`) passes through.
//!
//! **What F-L carries.** Its value is [`PalwCapacityLiabilityV1`]: the fence's height and the ramp's
//! schedule — steps `{ from_daa, rho, q_credit_permille }` (ADR §4.5, §6.1). The value is hashed whole
//! into `consensus_params_id` (the D1 rule: the value carries its own schedule; a later flag day
//! appends a step), every step's height is on the schedule the fork id reads, and the fold reads the
//! value through the `#[borsh(skip)]` mirror `PalwStateParamsV2::capacity_liability`
//! ([`crate::palw_state_v2::PalwStateParamsV2::capacity_step_at`] is the accessor the escrow and
//! shadow lanes read).
//!
//! **The producer side (AG-1…AG-5).**
//! * AG-1: the bond's posted collateral backs every one of its claims at once; nothing is ever debt
//!   (`slash_bond` saturates).
//! * AG-2: an INTENT-class conviction ([`palw_offence_is_intent_class_v1`]) forfeits, in one funnel,
//!   the whole posted collateral, every vesting row the bond is payee of that has not moved (its own
//!   rows burned whole, its seat legs in other producers' rows burned leg by leg), and every live
//!   claim of the bond (voided `AggregateForfeit`: its withheld reward is never minted).
//! * AG-3: the FIRST conviction of any producer or seat offence against a bond writes its freeze
//!   ([`PalwBondFreezeV1`], the rooted map `bond_freezes`). While present: the bond's attempts and
//!   free-prompt commitments are refused (`ProducerFrozen`, non-fatal for a block's own attempt), it
//!   is not drawn onto a panel (the draw's structural population skips it, base and eligible alike),
//!   a `Valid` it signs backs nothing (SR-6/SR-10), its exit gate is shut
//!   (`palw_bond_collateral_is_locked_v6`), and its own unlatched vesting rows are re-keyed to the
//!   freeze's lift so none matures under it. An intent-class freeze is FINAL (never lifted); a
//!   tier-capped freeze lifts at `since_daa + window_court` if no further conviction lands — removed
//!   by the end-of-block lift sweep, so "frozen" is the entry's presence and every reader (the draw,
//!   admission, the producer's facts, the exit gate) answers it from state alone.
//! * AG-4: only an attributed conviction reaches the pool; an unattributed failure costs the claim's
//!   own stage commitment, as it does today.
//! * AG-5: a claim voided past the fence stays convictable while its record and liability row stand
//!   (`claim_retirement_daa` ≥ `h_obl`), and a conviction of it takes this funnel.
//!
//! **The seat side (AS-1…AS-4)**, keyed on the claim's `accepted_daa`: the duty a seat reserves at
//! bind is [`palw_seat_duty_v2`] (`⌈λ/ρ⌉`, `⌈lock_2/ρ⌉`, capped by the commitment over the seats as
//! today — the ≤ 1 amplification bound holds), and the lock a counted `Valid` posts is
//! [`palw_seat_lock_v2`] (`max(1, ⌈lock/ρ⌉)` only once the step's credited attribution reaches
//! [`PALW_CAPACITY_Q_SEAT_PERMILLE_V1`]; otherwise today's lock). A `PanelFalseValidV2` conviction of
//! a seat takes the same funnel (AS-3); everything stays inside the one ledger (AS-4).

use crate::Hash64;
use crate::config::params::ForkActivation;
use crate::palw_offence_v1::PalwPanelContradictionV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, palw_rcore_duty_bind_v1};

/// The rule's version, written beside the fence's height in `consensus_params_id` (the rule rides its
/// fence: two builds arming different liability rules at one height announce different rulesets).
pub const PALW_CAPACITY_LIABILITY_DOMAIN_V1: &[u8] = b"misaka-palw/capacity/aggregate-liability/v1";

/// **One step of the ramp** (ADR-0160 §4.5, §6.1): from `from_daa` on, the seat side is re-priced by
/// the ramp factor `rho`, and `q_credit_permille` is the attribution rate the network credits
/// (`q_credit ≤ ½ × measured`, §10 D-8). The escrow lane reads the same step (`m_c = max(m*(q), ⌈E/ρ⌉)`).
///
/// Defined here because this lane owns the fence's value; the shadow lane's formulas module
/// (`palw_capacity_formulas_v1`) names the same type (re-export it from here on `rcore/cap-int`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCapacityStepV1 {
    /// The DAA score (the claim's `accepted_daa` for the seat side) from which the step applies.
    pub from_daa: u64,
    /// The ramp factor ρ ≥ 1 (1 = today's prices).
    pub rho: u32,
    /// The credited attribution rate, in permille (≤ 1000).
    pub q_credit_permille: u16,
}

/// **`Params::palw_capacity_aggregate_liability`'s value**: the fence's height and the ramp's schedule.
/// `steps` is non-empty, its first step starts AT the fence, `from_daa` strictly increases, every
/// `rho ≥ 1` and every `q_credit_permille ≤ 1000` ([`Self::refusal_v1`], which `validate_palw_v2`
/// runs). Hashed whole (the domain, the height, every step in order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityLiabilityV1 {
    pub activation: ForkActivation,
    pub steps: Vec<PalwCapacityStepV1>,
}

/// **At most eight steps** — the fence's height and seven later flag days (the ramp ADR-0160 §9 plans is
/// four: ρ = 10, 25, 100, 1000). Every step past the first is its own named fence in
/// `Params::palw_fences_v1` (`palw_capacity_aggregate_liability_step_2` … `_step_8`), so the fork id's
/// gate names each appended step's height: a node that did not append it is refused from that height,
/// not forked silently at it (the memory rule "a fence at a scheduled height is invisible to the fork id").
pub const PALW_CAPACITY_MAX_STEPS_V1: usize = 8;

/// One entry of a schedule written relative to the fence's height — what
/// [`PALW_T12_CAPACITY_STEPS_V1`] spells, so the drill's `--palw-drill-fence-at` (which moves the
/// height) moves the whole schedule with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCapacityStepOffsetV1 {
    /// DAA past the fence's height at which the step starts (the first entry is 0).
    pub after_daa: u64,
    pub rho: u32,
    pub q_credit_permille: u16,
}

/// **testnet-12's schedule** (ADR-0160 §9 stage 5, §10 D-9): the first step, ρ = 10, from the
/// fence's height. `q_credit` is 0 until Stage 0's adversarial runs measure an attribution rate and
/// the user credits half of it (D-8): at 0 the escrow lane keeps `m_c = E` and the seat lock keeps
/// today's price, so the only live change of this step is the seat duty ÷ρ. Later steps are appended
/// by later flag days, each with a drill that crosses it (one flag day per step, D-9).
pub const PALW_T12_CAPACITY_STEPS_V1: &[PalwCapacityStepOffsetV1] =
    &[PalwCapacityStepOffsetV1 { after_daa: 0, rho: 10, q_credit_permille: 0 }];

impl PalwCapacityLiabilityV1 {
    /// The value armed at `activation` over a schedule written relative to it.
    pub fn of_schedule_v1(activation: ForkActivation, schedule: &[PalwCapacityStepOffsetV1]) -> Self {
        let base = activation.daa_score();
        Self {
            activation,
            steps: schedule
                .iter()
                .map(|step| PalwCapacityStepV1 {
                    from_daa: base.saturating_add(step.after_daa),
                    rho: step.rho,
                    q_credit_permille: step.q_credit_permille,
                })
                .collect(),
        }
    }

    /// testnet-12's armed value at `activation` ([`PALW_T12_CAPACITY_STEPS_V1`]).
    pub fn t12_at_v1(activation: ForkActivation) -> Self {
        Self::of_schedule_v1(activation, PALW_T12_CAPACITY_STEPS_V1)
    }

    /// **What `validate_palw_v2` refuses in the value itself**: no step; a first step that does not
    /// start at the fence's height (a gap between the fence and its first step would be a height at
    /// which the fence is armed and prices nothing); heights not strictly increasing (one height, one
    /// step); `rho = 0` (a division by zero, and "cheaper than free"); `q_credit > 1000‰`.
    pub fn refusal_v1(&self) -> Option<&'static str> {
        let Some(first) = self.steps.first() else {
            return Some("palw_capacity_aggregate_liability names no step: an armed fence must say what it prices");
        };
        if first.from_daa != self.activation.daa_score() {
            return Some(
                "palw_capacity_aggregate_liability's first step does not start at the fence's height: between the two the fence \
                 would be armed and price nothing",
            );
        }
        if self.steps.len() > PALW_CAPACITY_MAX_STEPS_V1 {
            return Some("palw_capacity_aggregate_liability has more than eight steps: each later step is a named fence slot");
        }
        if self.steps.windows(2).any(|pair| pair[0].from_daa >= pair[1].from_daa) {
            return Some("palw_capacity_aggregate_liability's steps are not strictly increasing in from_daa: one height, one step");
        }
        if self.steps.iter().any(|step| step.rho == 0) {
            return Some("palw_capacity_aggregate_liability has a step with rho = 0: the ramp factor is at least 1 (1 = today)");
        }
        if self.steps.iter().any(|step| step.q_credit_permille > 1_000) {
            return Some("palw_capacity_aggregate_liability has a step crediting more than 1000‰ attribution");
        }
        None
    }
}

/// **Step `slot`'s height (1-based; slot 1 is the fence's own height) as a fence** — what
/// `Params::palw_fences_v1` lists under `palw_capacity_aggregate_liability_step_<slot>`. `None` where the
/// value is absent or has fewer steps.
pub fn palw_capacity_step_fence_v1(value: Option<&PalwCapacityLiabilityV1>, slot: usize) -> Option<ForkActivation> {
    value.and_then(|value| value.steps.get(slot.checked_sub(1)?).map(|step| ForkActivation::new(step.from_daa)))
}

/// **The step in force at `daa`** — `None` where the fence is `never()` or not yet active at `daa`,
/// else the last step whose `from_daa ≤ daa` (a valid value always has one: its first step starts at
/// the fence).
pub fn palw_capacity_step_at_v1(value: &PalwCapacityLiabilityV1, daa: u64) -> Option<PalwCapacityStepV1> {
    if value.activation == ForkActivation::never() || !value.activation.is_active(daa) {
        return None;
    }
    value.steps.iter().rev().find(|step| step.from_daa <= daa).copied()
}

// ---------------------------------------------------------------------------------------------
// The seat side (AS-1, AS-2)
// ---------------------------------------------------------------------------------------------

/// **AS-2's credit threshold `q_seat = E / (E + L_seat)`, in permille**, with `L_seat` the 3G tier a
/// seat conviction definitely collects (ADR §4.7, D-5). `G = g_res + E ≥ E`, so `E/(E + 3G) ≤ 1/4`:
/// 250‰ is the most any claim needs, and the seat lock is reduced only past it. (The whole-seat-bond
/// `L_seat` of D-5's located-fault case would lower it to ≈ 24‰; this lane credits the smaller `L`.)
pub const PALW_CAPACITY_Q_SEAT_PERMILLE_V1: u16 = 250;

/// **May AS-2 reduce the seat lock under `step`?** `q_credit ≥ q_seat`.
pub fn palw_seat_credit_applies_v1(step: &PalwCapacityStepV1) -> bool {
    step.q_credit_permille >= PALW_CAPACITY_Q_SEAT_PERMILLE_V1
}

fn ceil_over_rho(amount: u128, rho: u32) -> u128 {
    amount.div_ceil(u128::from(rho.max(1)))
}

/// **AS-1: the duty a seat reserves at bind** — `palw_rcore_duty_bind_v1(⌈λ/ρ⌉, ⌈lock_2/ρ⌉,
/// commitment, seats)` under a step, today's `palw_rcore_duty_bind_v1(λ, lock_2, commitment, seats)`
/// without one. `lock_2` is the UNREDUCED lock (AS-1 divides it whatever AS-2 says), `commitment` the
/// claim's commitment at bind (the escrow lane's `m_c + reserved` past its fence), so
/// `seats × duty ≤ commitment` (F14) holds at every ρ.
pub fn palw_seat_duty_v2(
    lambda_term: u128,
    lock_2: u128,
    commitment_at_bind: u128,
    seat_count: usize,
    step: Option<PalwCapacityStepV1>,
) -> u128 {
    match step {
        None => palw_rcore_duty_bind_v1(lambda_term, lock_2, commitment_at_bind, seat_count),
        Some(step) => palw_rcore_duty_bind_v1(
            ceil_over_rho(lambda_term, step.rho),
            ceil_over_rho(lock_2, step.rho),
            commitment_at_bind,
            seat_count,
        ),
    }
}

/// **AS-2: the lock one counted `Valid` posts** — `max(1 sompi, ⌈lock/ρ⌉)` under a step whose credit
/// applies ([`palw_seat_credit_applies_v1`]), else `lock` unchanged (no step, or a step below
/// `q_seat`). Never 0: B-3's exit gate reads a live lock's PRESENCE to pin the seat to its conviction
/// horizon.
pub fn palw_seat_lock_v2(lock: u128, step: Option<PalwCapacityStepV1>) -> u128 {
    match step {
        Some(step) if palw_seat_credit_applies_v1(&step) => ceil_over_rho(lock, step.rho).max(1),
        _ => lock,
    }
}

// ---------------------------------------------------------------------------------------------
// The freeze (AG-3) and the offence classes (AG-2, D-5)
// ---------------------------------------------------------------------------------------------

/// **A bond's freeze** (AG-3), rooted in `PalwChainStateV2::bond_freezes`: written by the first
/// conviction of any producer or seat offence against the bond past F-L, and rewritten by each later
/// one. `since_daa` the latest conviction's DAA; `offence_key` its consumed-offence key;
/// `forfeited_sompi` what the aggregate forfeitures of this bond took (collateral and burned vesting,
/// 0 for a tier freeze); `final_` whether an intent-class conviction forfeited the bond (never lifted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondFreezeV1 {
    pub since_daa: u64,
    pub offence_key: Hash64,
    pub forfeited_sompi: u64,
    pub final_: bool,
}

impl PalwBondFreezeV1 {
    /// The DAA at or past which the end-of-block sweep lifts a tier freeze (`since + window_court`);
    /// `None` for a final one.
    pub fn lifts_at_v1(&self, window_court: u64) -> Option<u64> {
        (!self.final_).then(|| self.since_daa.saturating_add(window_court))
    }
}

/// **Is `bond` frozen?** The entry's presence in the rooted map (AG-3): a tier freeze is removed by
/// the end-of-block lift sweep of the first block at or past its lift, so the fold, the draw on the
/// pre-object base, admission and the producer's facts on the parent all answer the same question
/// from state alone, with no clock of their own. Empty — `false` for every bond — on every chain
/// without F-L.
pub fn palw_bond_is_frozen_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> bool {
    state.bond_freeze_of_v1(bond).is_some()
}

/// `IdentityMismatch` — the committed execution answers another job or class (J-5).
pub const PALW_CONTRADICTION_IDENTITY_MISMATCH_V1: u8 = 9;
/// `OutputMismatch` — the committed output is not the one the execution generated (J-5).
pub const PALW_CONTRADICTION_OUTPUT_MISMATCH_V1: u8 = 10;
/// `ForgedOutputTiled` — a tiled class's committed token is not its row's selection (F1c).
pub const PALW_CONTRADICTION_FORGED_OUTPUT_TILED_V1: u8 = 11;
/// `LogitsNotStepOutput` — a logits row is not its step tree's head output (F1c).
pub const PALW_CONTRADICTION_LOGITS_NOT_STEP_OUTPUT_V1: u8 = 12;
/// `PromptNotAnchored` — an attempt's committed prompt is not its anchor's (F1-M).
pub const PALW_CONTRADICTION_PROMPT_NOT_ANCHORED_V1: u8 = 13;

/// **The borsh tag of a contradiction** (`PalwPanelContradictionV1`'s `use_discriminant`), by an
/// exhaustive match so a contradiction added later has to be classified here before this compiles.
pub fn palw_contradiction_tag_v1(contradiction: &PalwPanelContradictionV1) -> u8 {
    match contradiction {
        PalwPanelContradictionV1::ExecutorEquivocation(_) => 0,
        PalwPanelContradictionV1::CourtExecutorGuilty { .. } => 1,
        PalwPanelContradictionV1::ProducerWithholding { .. } => 2,
        PalwPanelContradictionV1::ConflictingPermit { .. } => 3,
        PalwPanelContradictionV1::CourtFraud { .. } => 4,
        PalwPanelContradictionV1::StepArithmetic { .. } => 5,
        PalwPanelContradictionV1::StepStructural(_) => 6,
        PalwPanelContradictionV1::Legs(_) => 7,
        PalwPanelContradictionV1::ForgedOutput { .. } => 8,
        PalwPanelContradictionV1::IdentityMismatch { .. } => PALW_CONTRADICTION_IDENTITY_MISMATCH_V1,
        PalwPanelContradictionV1::OutputMismatch { .. } => PALW_CONTRADICTION_OUTPUT_MISMATCH_V1,
        PalwPanelContradictionV1::ForgedOutputTiled { .. } => PALW_CONTRADICTION_FORGED_OUTPUT_TILED_V1,
        PalwPanelContradictionV1::LogitsNotStepOutput { .. } => PALW_CONTRADICTION_LOGITS_NOT_STEP_OUTPUT_V1,
        PalwPanelContradictionV1::PromptNotAnchored { .. } => PALW_CONTRADICTION_PROMPT_NOT_ANCHORED_V1,
    }
}

/// **What one bond was convicted of, as the aggregate funnel classifies it** — one entry per bond a
/// conviction charges (the accused, and for a kind-3 finding that acts on the claim, its producer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwConvictedOffenceV1 {
    /// A data-availability default (kind 5): the producer's S1.
    DaDefault,
    /// A proven court verdict recorded `CourtFraud` (kind 6).
    CourtFraud,
    /// A held dissection's verdict (kind 6, `CourtHeldVerdict`): it proves the producer's own filings
    /// false, not the committed execution (§4-ter, decision (B)).
    CourtHeldVerdict,
    /// An objective contradiction (kind 3 against a seat, kind 4 against the executor), by its tag.
    Contradiction { tag: u8 },
    /// An `ExecutorEquivocation` (kind 0); Eq keeps its t12 cap (D-5).
    Equivocation,
    /// A covering signer's S4 inside a DA default (DA-7 step 1).
    CoveringSigner,
}

/// **The intent class** (ADR-0160 §4.6 AG-2, D-5): the convictions that forfeit the WHOLE bond — DA
/// default (S1), `IdentityMismatch` (9), `OutputMismatch` (10), `ForgedOutputTiled` (11),
/// `PromptNotAnchored` (13) and a proven court verdict (`CourtFraud`). Everything else keeps the
/// ADR-0152 §3.6 tier (capped at 3G) plus a lifting freeze: `StepArithmetic` (5) and
/// `LogitsNotStepOutput` (12), which an engine non-determinism could produce on an honest node; the
/// structural and flat-output refutations (6, 7, 8); the named voids a kind 3 restates (2, 4); a
/// held dissection's verdict; Eq; a covering signer's S4. The same contradiction classes a seat's
/// `PanelFalseValidV2` (AS-3): a `Valid` on a claim answering another job, output or prompt is
/// attributable intent, one on an arithmetic fault is not (yet — D-5's burn-in extends the class).
pub fn palw_offence_is_intent_class_v1(offence: PalwConvictedOffenceV1) -> bool {
    match offence {
        PalwConvictedOffenceV1::DaDefault | PalwConvictedOffenceV1::CourtFraud => true,
        PalwConvictedOffenceV1::Contradiction { tag } => matches!(
            tag,
            PALW_CONTRADICTION_IDENTITY_MISMATCH_V1
                | PALW_CONTRADICTION_OUTPUT_MISMATCH_V1
                | PALW_CONTRADICTION_FORGED_OUTPUT_TILED_V1
                | PALW_CONTRADICTION_PROMPT_NOT_ANCHORED_V1
        ),
        PalwConvictedOffenceV1::CourtHeldVerdict | PalwConvictedOffenceV1::Equivocation | PalwConvictedOffenceV1::CoveringSigner => false,
    }
}

/// **A kind-3 or kind-4 conviction's class, read off its evidence** — the contradiction the adjudicator
/// admitted (`PalwPanelFalseValidEvidenceV2` / `PalwExecutorRefutedEvidenceV1`). Evidence that does
/// not decode (which the adjudicator refused before the funnel is reached) and every other kind
/// classify as the tier (an unknown tag): the funnel never forfeits a bond on bytes it cannot read.
pub fn palw_evidence_contradiction_offence_v1(
    kind: crate::palw_offence_v1::PalwOffenceKindV1,
    evidence: &[u8],
) -> PalwConvictedOffenceV1 {
    use crate::palw_offence_attribution_v1::{PalwExecutorRefutedEvidenceV1, PalwPanelFalseValidEvidenceV2};
    use crate::palw_offence_v1::PalwOffenceKindV1;
    let tag = match kind {
        PalwOffenceKindV1::PanelFalseValidV2 => {
            borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok().map(|payload| palw_contradiction_tag_v1(&payload.contradiction))
        }
        PalwOffenceKindV1::ExecutorRefuted => {
            borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok().map(|payload| palw_contradiction_tag_v1(&payload.contradiction))
        }
        _ => None,
    };
    PalwConvictedOffenceV1::Contradiction { tag: tag.unwrap_or(u8::MAX) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSK: u128 = 100_000_000;

    fn step(from_daa: u64, rho: u32, q: u16) -> PalwCapacityStepV1 {
        PalwCapacityStepV1 { from_daa, rho, q_credit_permille: q }
    }

    /// **The schedule: the step in force at a DAA, the fence's own height, and the value's refusals.**
    #[test]
    fn the_step_in_force_is_the_last_one_at_or_below_the_daa_and_none_below_the_fence() {
        let value = PalwCapacityLiabilityV1 {
            activation: ForkActivation::new(500),
            steps: vec![step(500, 10, 150), step(2_000, 25, 150), step(9_000, 100, 200)],
        };
        assert_eq!(value.refusal_v1(), None);
        assert_eq!(palw_capacity_step_at_v1(&value, 499), None, "dormant below the fence");
        assert_eq!(palw_capacity_step_at_v1(&value, 500), Some(step(500, 10, 150)), "the first step at the fence");
        assert_eq!(palw_capacity_step_at_v1(&value, 1_999), Some(step(500, 10, 150)));
        assert_eq!(palw_capacity_step_at_v1(&value, 2_000), Some(step(2_000, 25, 150)), "an appended step from its own height");
        assert_eq!(palw_capacity_step_at_v1(&value, u64::MAX), Some(step(9_000, 100, 200)));
        let never = PalwCapacityLiabilityV1 { activation: ForkActivation::never(), steps: vec![step(u64::MAX, 10, 0)] };
        assert_eq!(palw_capacity_step_at_v1(&never, u64::MAX), None, "never() arms nothing, at any height");
        // testnet-12's value: ρ = 10 from the fence, nothing credited.
        let t12 = PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::new(1_234));
        assert_eq!(t12.steps, vec![step(1_234, 10, 0)]);
        assert_eq!(t12.refusal_v1(), None);
        // The refusals, each by name.
        let bad = |steps: Vec<PalwCapacityStepV1>| PalwCapacityLiabilityV1 { activation: ForkActivation::new(500), steps }.refusal_v1();
        assert!(bad(vec![]).unwrap().contains("no step"));
        assert!(bad(vec![step(501, 10, 0)]).unwrap().contains("first step"));
        assert!(bad(vec![step(500, 10, 0), step(500, 25, 0)]).unwrap().contains("strictly increasing"));
        assert!(bad(vec![step(500, 10, 0), step(400, 25, 0)]).unwrap().contains("strictly increasing"));
        assert!(bad(vec![step(500, 0, 0)]).unwrap().contains("rho = 0"));
        assert!(bad(vec![step(500, 10, 1_001)]).unwrap().contains("1000‰"));
        let nine: Vec<_> = (0..9).map(|i| step(500 + i, 10, 0)).collect();
        assert!(bad(nine).unwrap().contains("more than eight"));
        assert_eq!(palw_capacity_step_fence_v1(Some(&value), 2), Some(ForkActivation::new(2_000)), "slot 2 is the second step");
        assert_eq!(palw_capacity_step_fence_v1(Some(&value), 4), None);
        assert_eq!(palw_capacity_step_fence_v1(None, 1), None);
        assert_eq!(bad(vec![step(500, 1, 1_000)]), None, "ρ = 1 and full credit are legal (today's prices, a measured q)");
    }

    /// **A-I5: `seats × duty′ ≤ commitment′` and `lock′ ≥ 1 sompi`, at every ρ and credit** — and with
    /// no step both are today's functions, byte for byte.
    #[test]
    fn a_i5_the_amplification_bound_holds_and_the_lock_never_reaches_zero() {
        let cases: &[(u128, u128, u128, usize)] = &[
            (256 * MSK, 160 * MSK, 3_201 * MSK, 5),
            (100 * MSK, 459 * MSK, 3_225 * MSK, 5),
            (100 * MSK, 33_379 * MSK, 62_943 * MSK, 5),
            (1, 1, 1, 5),
            (0, 0, 0, 0),
            (7, 3, 2, 3),
        ];
        for &(lambda, lock_2, commitment, seats) in cases {
            assert_eq!(palw_seat_duty_v2(lambda, lock_2, commitment, seats, None), palw_rcore_duty_bind_v1(lambda, lock_2, commitment, seats));
            assert_eq!(palw_seat_lock_v2(lock_2, None), lock_2, "no step: today's lock");
            for rho in [1u32, 2, 10, 25, 50, 100, 1_000, u32::MAX] {
                for q in [0u16, 142, 249, 250, 500, 1_000] {
                    let s = Some(step(0, rho, q));
                    let duty = palw_seat_duty_v2(lambda, lock_2, commitment, seats, s);
                    assert!(duty * (seats.max(1) as u128) <= commitment, "F14 (seats × duty′ ≤ commitment′) at ρ={rho}");
                    assert!(duty <= palw_rcore_duty_bind_v1(lambda, lock_2, commitment, seats), "ρ never raises the duty");
                    let lock = palw_seat_lock_v2(lock_2, s);
                    if q >= PALW_CAPACITY_Q_SEAT_PERMILLE_V1 {
                        assert!(lock >= 1, "lock′ ≥ 1 sompi");
                        assert_eq!(lock, lock_2.div_ceil(u128::from(rho)).max(1));
                    } else {
                        assert_eq!(lock, lock_2, "below q_seat the lock is today's");
                    }
                }
            }
        }
        // The shipped floor's shape at ρ = 100 (§4.7: "a floor duty is ≈ 6.4"): the commitment over
        // five seats no longer binds, ⌈λ/ρ⌉ vs ⌈lock_2/ρ⌉ does.
        let floor = palw_seat_duty_v2(640 * MSK, 240 * MSK, 3_201 * MSK, 5, Some(step(0, 100, 0)));
        assert_eq!(floor, 64 * MSK / 10, "⌈640.00/100⌉ MSK");
    }

    /// **The intent class, by name** — the contract's list and nothing else.
    #[test]
    fn the_intent_class_is_the_da_default_the_court_fraud_and_contradictions_9_10_11_13() {
        use PalwConvictedOffenceV1 as O;
        assert!(palw_offence_is_intent_class_v1(O::DaDefault));
        assert!(palw_offence_is_intent_class_v1(O::CourtFraud));
        for tag in 0..=20u8 {
            let intent = palw_offence_is_intent_class_v1(O::Contradiction { tag });
            assert_eq!(intent, matches!(tag, 9 | 10 | 11 | 13), "tag {tag}");
        }
        for tier in [O::CourtHeldVerdict, O::Equivocation, O::CoveringSigner, O::Contradiction { tag: 5 }, O::Contradiction { tag: 12 }] {
            assert!(!palw_offence_is_intent_class_v1(tier), "{tier:?} keeps the 3G tier");
        }
    }

    /// The freeze's lift: `since + window_court` for a tier freeze, never for a final one.
    #[test]
    fn a_tier_freeze_lifts_at_since_plus_window_court_and_a_final_one_never() {
        let tier = PalwBondFreezeV1 { since_daa: 700, offence_key: Hash64::default(), forfeited_sompi: 0, final_: false };
        assert_eq!(tier.lifts_at_v1(3_000), Some(3_700));
        assert_eq!(PalwBondFreezeV1 { final_: true, ..tier }.lifts_at_v1(3_000), None);
        assert_eq!(PalwBondFreezeV1 { since_daa: u64::MAX - 1, ..tier }.lifts_at_v1(3_000), Some(u64::MAX), "saturating");
    }

    // ---- L-T6: the EV golden (ADR-0160 §4.5) ----------------------------------------------------

    /// `m*(q) = max(0, ⌈(P*·E − q·L/(1−q)) / (1−P*)⌉)` at `P* = ½`, in sompi — the escrow lane's
    /// `palw_monetary_prelicense_risk_v1`, restated here only to hold the EV claim this lane's freeze
    /// earns (the freeze is what makes one claim the binding case).
    fn m_star(e: f64, q: f64, l: f64) -> f64 {
        (e - 2.0 * q * l / (1.0 - q)).max(0.0)
    }

    /// `EV(K) = (1−q)^K · K · [P·E − (1−P)·m] − (1 − (1−q)^K) · L` — a K-claim campaign inside one
    /// maturity window earns only if no claim is convicted (freeze-on-first-conviction burns every
    /// unmatured reward of the bond).
    fn ev(k: u32, q: f64, p: f64, e: f64, m: f64, l: f64) -> f64 {
        let survive = (1.0 - q).powi(k as i32);
        survive * f64::from(k) * (p * e - (1.0 - p) * m) - (1.0 - survive) * l
    }

    /// **L-T6: over K ∈ {1, 2, 10, 100, 1000} × q × P, EV ≤ 0 at every P ≤ P* once m ≥ m*(q)** — with
    /// L = 3G = 9,602.9 MSK (the normative loss) and L = 13,000 MSK (a 13k bond's whole forfeiture),
    /// at the ramp's `m = max(m*(q), ⌈E/ρ⌉)` for every ρ. And the ADR's Appendix A row, reproduced:
    /// q = 0.11, P = 0.5, m = 32, L = 13,000 gives −20, −193, −1,317, −4,006, −12,999, −13,000.
    #[test]
    fn l_t6_the_campaign_ev_is_never_positive_at_p_up_to_p_star() {
        let e = 3_200.8465;
        for l in [9_602.9, 13_000.0] {
            for q in [0.0, 0.05, 0.08, 0.10, 0.11, 0.13, 0.143, 0.2, 0.3, 0.5] {
                for rho in [1.0, 10.0, 25.0, 50.0, 100.0, 1_000.0] {
                    let m = m_star(e, q, l).max((e / rho as f64).ceil());
                    for p in [0.0, 0.1, 0.25, 0.4, 0.5] {
                        for k in [1u32, 2, 10, 100, 1_000] {
                            let v = ev(k, q, p, e, m, l);
                            assert!(v <= 1e-6, "EV(K={k}) = {v} > 0 at q={q} P={p} ρ={rho} m={m} L={l}");
                        }
                    }
                }
            }
        }
        let appendix: Vec<f64> = [1u32, 2, 5, 10, 100, 1_000].iter().map(|k| ev(*k, 0.11, 0.5, e, 32.0, 13_000.0).round()).collect();
        assert_eq!(appendix, vec![-20.0, -193.0, -1_317.0, -4_006.0, -12_999.0, -13_000.0], "ADR-0160 Appendix A");
    }
}
