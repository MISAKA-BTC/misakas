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
//!   the whole posted collateral, the bond's own unmatured reward — its legs of every vesting row that
//!   has not moved: the producer leg of each row it produced, its seat legs in other producers' rows;
//!   every other payee of those rows keeps its leg — and every live claim of the bond (voided
//!   `AggregateForfeit`: its withheld reward is never minted, and a challenger's held forfeit on it
//!   is refunded, since the void closes both of that forfeit's refund doors). The conviction's
//!   record keeps the TIER debit as its `collected`, so the reporter reward is on the tier as today,
//!   never on the forfeiture (which the freeze's `forfeited_sompi` records, with any held-forfeit
//!   refund that reaches the bond after it — a final freeze's bond posts no collateral).
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
//! bind is [`palw_seat_duty_v2`] (`⌈λ/ρ⌉` and the lock the licence will post, capped by the commitment
//! over the seats as today — the ≤ 1 amplification bound holds, and a bound seat is backed at the
//! licence by construction), and the lock a counted `Valid` posts is [`palw_seat_lock_v2`]
//! (`max(1, ⌈lock/ρ⌉)` only once the step's credited attribution reaches
//! [`PALW_CAPACITY_Q_SEAT_PERMILLE_V1`]; otherwise today's lock). A `PanelFalseValidV2` conviction of
//! a seat takes the same funnel (AS-3); everything stays inside the one ledger (AS-4).
//!
//! **What the ramp's credit may count** ([`palw_producer_conviction_credits_q_v1`]): the step's
//! `q_credit_permille` is half a MEASURED rate of producer convictions that forfeit the whole bond
//! (the intent class) — never of tier-class ones, which collect only ADR-0152's S2 tier (at a 13k
//! bond far below the `L = 3G` the escrow lane's `m*(q)` assumes) and burn nothing.

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
    /// The credited attribution rate, in permille (≤ 1000): at most half the rate, measured by
    /// adversarial runs (D-8), at which a fraudulent claim's producer is convicted by a route that
    /// forfeits its whole bond — the routes [`palw_producer_conviction_credits_q_v1`] names. A
    /// tier-class producer conviction is NOT counted: it collects the S2 tier alone.
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
/// today's price, so the only live change of this step is the seat duty's λ-term ÷ρ — the duty falls
/// from 640.17 to the lock it must back (240.13 MSK, [`palw_seat_duty_v2`]) and the network's floor
/// seat capital moves ×1.07 (0.94 → 1.00 claims/DAA, L-T4), not ×10, until a later step credits
/// q ≥ [`PALW_CAPACITY_Q_SEAT_PERMILLE_V1`]. Later steps are appended by later flag days, each with a
/// drill that crosses it (one flag day per step, D-9).
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
/// 250‰ is the most any claim needs, and the seat lock is reduced only past it.
///
/// **Why not D-5's ≈ 24‰ (the whole seat bond, ≥ 130,000 MSK, as `L_seat`).** `L_seat` must be what
/// the CHEAPEST route that convicts a fraud-backing seat collects, and for the naive fraud — roots
/// with no material behind them, which the producer can only default on — that route is DA-7's
/// covering-signer S4 ([`PalwConvictedOffenceV1::CoveringSigner`]: lock + `min(25%·C, 3G)`, the tier).
/// It fires automatically at the DA deadline and takes the same (seat, claim) ledger key a later
/// `PanelFalseValidV2` would, so the seat is charged once, at the tier. Whole-bond forfeiture of a
/// seat for an `IdentityMismatch`/`OutputMismatch`/`ForgedOutputTiled`/`PromptNotAnchored` finding
/// therefore does not raise the minimum `L_seat` above 3G; 24‰ would also need covering signers in
/// the intent class — the honest-offline seat's risk, a user decision (lane liab review, finding 3).
///
/// **So the seat side's ×ρ is conditional**: at testnet-12's first step (ρ = 10, q_credit = 0,
/// [`PALW_T12_CAPACITY_STEPS_V1`]) only the duty's λ-term divides and the lock stays today's — the
/// duty then stops at the lock it must back ([`palw_seat_duty_v2`]) — so the eight genesis seats'
/// network floor rate goes 0.94 → 1.00 claims/DAA (×1.07, L-T4), not ×10; the ×ρ rows
/// (9.4 / 23.5 / 94) need a step crediting q ≥ 250‰, which D-8's half rule reaches only from a
/// measured attribution rate of 500‰.
pub const PALW_CAPACITY_Q_SEAT_PERMILLE_V1: u16 = 250;

/// **May AS-2 reduce the seat lock under `step`?** `q_credit ≥ q_seat`.
pub fn palw_seat_credit_applies_v1(step: &PalwCapacityStepV1) -> bool {
    step.q_credit_permille >= PALW_CAPACITY_Q_SEAT_PERMILLE_V1
}

fn ceil_over_rho(amount: u128, rho: u32) -> u128 {
    amount.div_ceil(u128::from(rho.max(1)))
}

/// **AS-1: the duty a seat reserves at bind** — `palw_rcore_duty_bind_v1(⌈λ/ρ⌉, lock′, commitment,
/// seats)` under a step, with `lock′ = `[`palw_seat_lock_v2`]`(lock_2, step)` — the lock a counted
/// `Valid` on this claim will post at the licence (AS-2); today's `palw_rcore_duty_bind_v1(λ, lock_2,
/// commitment, seats)` without one. `lock_2` is today's (unreduced) price, `commitment` the claim's
/// commitment at bind (the escrow lane's `m_c + reserved` past its fence), so `seats × duty ≤
/// commitment` (F14) holds at every ρ.
///
/// **Why the lock term is AS-2's lock, not `⌈lock_2/ρ⌉`** (lane liab review 2, finding 3). The bind
/// admits a seat on `room ≥ max(duty′, lock′)` but reserves only `duty′`; the licence takes the top-up
/// `lock′ − duty′` from whatever room is left then (`backed_at`). Dividing `lock_2` inside the duty
/// while AS-2 keeps today's lock (a step below [`PALW_CAPACITY_Q_SEAT_PERMILLE_V1`], testnet-12's
/// first step among them) made a floor duty 64.02 MSK against a 240.13 MSK lock: a seat near its
/// ceiling was bound on several claims its room could not back, their `Valid`s went unbacked, and the
/// honest producer's second `ReceiptTimeout` charged its stage commitment (A4 turned into honest
/// S0′ losses). With `lock′` as the lock term, `duty′ ≥ lock′` whenever `commitment / seats ≥ lock′`
/// — every floor and 8k claim while the commitment is `w + E` — so a bound seat is backed at the
/// licence by construction, as L-4b makes it below the fence; the ×ρ on the lock term arrives with
/// AS-2's credit. (Past the escrow lane's credit the commitment can fall under `seats × lock′`; that
/// lane's bind then reserves the eligibility itself.) With no step this is today's duty, byte for byte.
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
            palw_seat_lock_v2(lock_2, Some(step)),
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
/// 0 for a tier freeze) — plus any held-forfeit refund that reached the bond after it was forfeited,
/// which joins the forfeiture instead of re-posting collateral (lane liab review 2, finding 1);
/// `final_` whether an intent-class conviction forfeited the bond (never lifted; a final freeze's bond
/// posts no collateral, a load invariant).
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
///
/// **S1 here is the ADR's rule and an OPEN USER DECISION before F-L is armed** (lane liab review,
/// finding 2). A DA default is how a fraud with no material behind its roots is convicted (the
/// producer can only withhold), so the ρ ramp's `EV ≤ 0` (§4.5: one conviction loses at least
/// `L = 3G` and every unmatured reward — true of this class only,
/// [`palw_producer_conviction_credits_q_v1`]) rests on S1 reaching this funnel. But the same default
/// is what an honest producer whose node is down through one `W_disclose` (1,200 DAA ≈ 40 h on
/// testnet-12) incurs — operator nodes accuse "blind" every claim no operator can replay
/// (free-prompt, C7, undeclared classes) — and past F-L it then loses its whole bond, where below it
/// loses the claim's commitment.
/// Consensus cannot tell the two apart. The reporter reward stays on the tier debit either way
/// (`close_conviction_v1`), so forcing a default is not a bounty on the bond.
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

/// **Does a PRODUCER conviction by `offence` count toward the attribution rate a ramp step may
/// credit (`PalwCapacityStepV1::q_credit_permille`)?** Exactly the intent class (ADR-0160 §4.5, D-8;
/// lane liab review 2, finding 4).
///
/// The escrow lane's `m*(q) = max(0, E − 2·q·L/(1−q))` is safe only if every conviction the credited
/// `q` counts collects at least the `L` it is computed with (`3G` = 9,602.9 MSK on the floor). An
/// intent-class conviction does: AG-2 takes the whole posted collateral — at least the 13,000 MSK
/// producer floor — every live claim and the bond's own unmatured legs. A tier-class producer
/// conviction does NOT: `StepArithmetic` (5), the structural and flat-output refutations (6, 7, 8),
/// `LogitsNotStepOutput` (12) and a held dissection's verdict take ADR-0152's S2/S3 tier alone — on a
/// live claim `reserved + E + min(10%·C₀, 3G)` (plus the court time for a verdict), 1,300 MSK of
/// action at a 13k bond; after `Final` the row and `min(25%·C₀, 3G)` — and their freeze burns
/// nothing. At the ramp's gate (`q = 0.143`, ρ = 10, `m = ⌈E/ρ⌉`) a 13k garbage claim convictable
/// only that way has `EV(1) > 0` (L-T6, measured on the fold). A court DEFAULT (`CourtDefault`)
/// writes no conviction record and never reaches the funnel, so it does not count either; nor does
/// an equivocation (it convicts no claim) or a covering signer's S4 (a seat's).
///
/// So the adversarial runs that measure `q` (§9 Stage 0, the shadow lane's measured rate) count a
/// fraudulent claim as attributed only when its producer is convicted by one of these routes before
/// any reward of its bond matures; a step whose `m_c` is to reach `⌈E/ρ⌉` needs that rate ≥ 0.29.
/// (The seat side needs no such restriction: every seat route takes the seat's lock plus
/// `min(25%·C₀, 3G)`, and a bond is seated only while it posts the 130,000 MSK seat floor, over
/// 13 × 3G.)
pub fn palw_producer_conviction_credits_q_v1(offence: PalwConvictedOffenceV1) -> bool {
    palw_offence_is_intent_class_v1(offence)
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
    /// no step both are today's functions, byte for byte. **And L-4b at every step** (lane liab review
    /// 2, finding 3): the duty a bound seat reserves covers the lock its `Valid` will post whenever the
    /// commitment over the seats does (`duty′ ≥ min(lock′, commitment′ / seats)`), so the licence's
    /// top-up `lock′ − duty′` is 0 on every floor and 8k claim at every ρ and credit.
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
                    assert!(
                        duty >= lock.min(commitment / (seats.max(1) as u128)),
                        "L-4b: the duty covers the lock the licence posts wherever the commitment allows (ρ={rho}, q={q})"
                    );
                    assert_eq!(
                        duty,
                        palw_rcore_duty_bind_v1(lambda.div_ceil(u128::from(rho)), lock, commitment, seats),
                        "AS-1's lock term is AS-2's lock (ρ={rho}, q={q})"
                    );
                }
            }
        }
        // The shipped floor's shape at ρ = 100 (§4.7: "a floor duty is ≈ 6.4"), once AS-2's credit
        // applies: the commitment over five seats no longer binds, ⌈λ/ρ⌉ vs ⌈lock_2/ρ⌉ does.
        let floor = palw_seat_duty_v2(640 * MSK, 240 * MSK, 3_201 * MSK, 5, Some(step(0, 100, 250)));
        assert_eq!(floor, 64 * MSK / 10, "⌈640.00/100⌉ MSK");
        // Below q_seat (testnet-12's first step: nothing credited) the lock stays 240, and so does the
        // duty's floor under it: the λ-term alone divides — 640 → 240, not 6.4.
        let uncredited = palw_seat_duty_v2(640 * MSK, 240 * MSK, 3_201 * MSK, 5, Some(step(0, 100, 0)));
        assert_eq!(uncredited, 240 * MSK, "max(⌈640/100⌉, 240) MSK: the duty backs the lock the licence posts");
        assert_eq!(palw_seat_lock_v2(240 * MSK, Some(step(0, 100, 0))), uncredited, "top-up 0 at the licence");
        // Review 2's measured case (ρ = 10, q = 0, the floor's 640.17 / 240.13 MSK): the old AS-1
        // (`⌈lock_2/ρ⌉` as the lock term) reserved 64.02 and left the licence a 176.11 MSK top-up to
        // find; the lock term closes it.
        let (lambda, lock) = (64_017 * MSK / 100, 24_013 * MSK / 100);
        let old_rule = palw_rcore_duty_bind_v1(lambda.div_ceil(10), lock.div_ceil(10), 3_201 * MSK, 5);
        assert_eq!(old_rule, lambda.div_ceil(10), "the old AS-1: 64.02 MSK against a 240.13 MSK lock");
        assert_eq!(palw_seat_duty_v2(lambda, lock, 3_201 * MSK, 5, Some(step(0, 10, 0))), lock, "now: the lock itself");
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

    /// **L-T6's premise, per route (lane liab review 2, finding 4): the L above is only what a
    /// WHOLE-BOND producer conviction takes, so only those may be counted in q.** At a 13k piece on
    /// the floor, an intent-class conviction takes the piece (13,000 ≥ 3G) and a tier-class one the S2
    /// tier on a live claim, `w + E + min(10%·C₀, 3G)` = 4,501 MSK before the court time (1,620 with
    /// the escrow lane's `m_c = ⌈E/ρ⌉` in the escrow slot) — under 3G, and at the ramp's gate (q =
    /// 0.143, ρ = 10) EV(1) > 0 with it. The consensus tests measure the same routes on the fold
    /// (`l_t6_measured_one_conviction_at_13k_by_route_and_only_whole_bond_routes_credit_q`).
    #[test]
    fn l_t6_only_whole_bond_producer_convictions_may_be_counted_in_q() {
        use PalwConvictedOffenceV1 as O;
        let (e, w, c0): (f64, f64, f64) = (3_200.8465, 0.1075266, 13_000.0);
        let three_g = 3.0 * (e + w);
        let (q, rho): (f64, f64) = (0.143, 10.0);
        let m = m_star(e, q, three_g).max((e / rho).ceil());
        let s2 = |escrow: f64| w + escrow + (0.1 * c0).min(three_g);
        assert!(ev(1, q, 0.5, e, m, c0) <= 0.0, "the whole piece: EV(1) ≤ 0 at the gate");
        assert!(s2(e) < three_g && ev(1, q, 0.5, e, m, s2(e)) > 0.0, "the S2 tier alone: under 3G, EV(1) > 0");
        assert!(ev(1, q, 0.5, e, m, s2((e / rho).ceil())) > 0.0, "and with the escrow lane's m_c in the slot");
        for offence in [O::DaDefault, O::CourtFraud, O::Contradiction { tag: 9 }, O::Contradiction { tag: 13 }] {
            assert!(palw_producer_conviction_credits_q_v1(offence), "{offence:?} forfeits the whole bond: counted");
        }
        for offence in [
            O::CourtHeldVerdict,
            O::Equivocation,
            O::CoveringSigner,
            O::Contradiction { tag: 5 },
            O::Contradiction { tag: 6 },
            O::Contradiction { tag: 7 },
            O::Contradiction { tag: 8 },
            O::Contradiction { tag: 12 },
            O::Contradiction { tag: u8::MAX },
        ] {
            assert!(!palw_producer_conviction_credits_q_v1(offence), "{offence:?} takes the tier: not counted");
        }
    }
}
