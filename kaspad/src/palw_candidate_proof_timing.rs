//! **A `Candidate` class is proved for its own admission audit** (the Activation Pool research's P3,
//! approved by the user on 2026-09-25; node policy only) — what the panel's `readiness_duties` asks
//! first for every class, before today's duty and M1's escalation.
//!
//! **The hole.** A `Candidate` reads its seats' readiness at one span only: its admission audit `S`
//! (ADR-0147's jury — one audit a period, the fold's own predicate, [`palw_candidate_audit_due_v1`]).
//! The Activation Pool's preparation reward (a) (on its own line, `palw_activation_pool`) pays a
//! drawn juror there only if its latest proof LANDED at `S − 2` or earlier. The seat re-proved every
//! row on the half-age cadence (`palw_readiness_duty_due_v2`, every few DAA on testnet-12) with no
//! regard to the audit, so a quarter to a third of the audits (measured below) found a prepared
//! juror's latest proof landed at `S − 1` — unpaid, and at the audit that seats the class, never paid
//! again. And every Candidate row cost a proof of tens of KB every few DAA (M1's block space) to keep
//! fresh a row nothing reads between audits.
//!
//! **The clock** (the node's own): a proof is sent at the virtual DAA `v` and names `v`'s span; the
//! block being mined carries it, or the next one, and the chain block after that accepts it — by
//! `v + 1` at the fastest and by `v + 2` at the carriage M1 plans for
//! ([`PALW_READINESS_ESCALATION_LANDING_DAA_V1`]). A block's registry step reads the rows its parent
//! left; the node reads a block's step one DAA after it (its virtual is the block's DAA + 1).
//!
//! **The window** ([`palw_candidate_proof_window_v1`]) is what two cuts leave. **(a)'s**: a proof sent
//! at `S − 4` lands by `S − 2` at M1's carriage, one sent at `S − 3` lands at `S − 1` — so `S − 4` is
//! the last. **The admission's**: the audit may admit the class (`Candidate → Prefetching`, and with
//! `ready_enough` on to `Probation` at `S + 1`, whose step reads `panel_drawable` every span); the
//! node reads that at `S + 1` and its ordinary duty sent then is accepted by `S + 3` at M1's carriage
//! — so the row the audit read has to stand through block `S + 3`, which an eight-span row dated
//! `S − 5` does and one dated `S − 6` does not: every prepared seat's row would lapse at once at
//! `S + 3`, a HELD for a class that went straight on to `Probation` (measured below). So on
//! testnet-12 (one-DAA spans, eight-span rows, a 100-DAA audit period), for each audit `S`:
//!
//! * **`S − 5`: the aligned proof** — it lands at `S − 4` at the fastest, `S − 3` at M1's carriage,
//!   and still by `S − 2` a block late; its row stands at the audit and through the admission.
//!   **One submitted proof per audit**: a proof that was submitted and has not landed is not copied
//!   (the copy could land at `S − 1` and override it — the loss this closes); only a proof the
//!   mempool refused, or a tick with no carrier slot, is retried, at `S − 4` and never later.
//! * **`S − 3` … `S − 1`: nothing for the class** — not today's duty and not M1's escalation. A proof
//!   sent at `S − 3` lands at `S − 1` at M1's carriage, one sent at `S − 2` at the fastest.
//! * **`S`: the hand-off**, only by a seat whose row serves this audit (it is prepared): one ordinary
//!   proof at the audit span itself, when nothing it carries can land before the audit's block. If the
//!   audit admits the class, today's duty takes the row from `S + 1` with the aligned row standing
//!   through `S + 3` — one escalation's chance at M1's carriage; the hand-off, landing by `S + 2`, is
//!   the second, which today's cadence always has (its half-age proof, then its escalation). Its row
//!   is five spans old at `S`, not lapsing, so it takes no escalated site and no pool reserve.
//! * **Between audits: nothing** — nothing reads a Candidate's row there.
//!
//! So a Candidate costs a prepared seat two proofs a period and an unprepared one one, where today's
//! cadence spends twenty to twenty-nine on a row (measured below on testnet-12's clock, M1's copy
//! guard armed).
//!
//! **M1 and P3 — which wins: P3, for a Candidate, whether AND where.** A Candidate's proof is sent only
//! as above, and always at the Own site, never M1's escalated one — not outside its window, where its
//! row lapses and nothing reads it before the audit (a proof hurried into `S − 3 … S − 1` would cost
//! the juror's (a)), and not inside it either. M1's escalated site — one proof a tick, never two
//! slots running — is the guarantee that keeps a COUNTED row from lapsing under a DA storm; a
//! Candidate's proof there (its row long lapsed, M1 would escalate it) could take the slot the tick
//! before a counted row needs it. So a Candidate row about to lapse does not escalate; at the Own
//! site its proof goes behind every proof M1 hurries and ahead of the ones it does not hurry yet
//! ([`palw_candidate_own_order_v1`]), so a counted row that is not lapsing does not starve its
//! two-span window (under a sustained single-seat DA storm the court still takes every other slot:
//! (a) paid at 160 and 132 of 176 audits at the one- and two-block carriages, measured below). **Every
//! class that is not `Candidate` — and every class on a network without R-core+ — gets
//! [`PalwCandidateProofPlanV1::Today`]: today's duty and M1's urgency exactly as they were**, and no
//! counted row lapses for P3. (Where a miner's pool puts the proof once sent is M1's pool rule, which
//! reads the tip row and not the lifecycle — unchanged: a Candidate's aligned proof renews a lapsed
//! row, so it heads a template only after every lapsing row; its hand-off renews a row that is not
//! lapsing, so it heads none. That is also why the aligned proof is not dated `S − 6` with the
//! hand-off to carry the admission: the hand-off's row would be lapsing at `S`, and every prepared
//! seat's would take the pool's head at once, ahead of counted rows lapsing a span later.)
//!
//! **Policy, never a rule.** Nothing in block validation or the fold reads this module; no id, root
//! or fingerprint moves. Past R-core+ only (testnet-12), with the twin below.
//!
//! **R2** (the Activation Pool line, `palw_activation_pool`): past it the fold audits each Candidate
//! at its OWN staggered span of the period (`palw_admission_audit_due_staggered_v1`). That line is not
//! on this branch; [`palw_candidate_audit_due_v1`] asks the fold's predicate as the fold asks it here
//! (ADR-0147's unstaggered one), and `the_node_asks_the_folds_own_audit_predicate` fails at the merge
//! that brings R2 until it asks R2's too. The schedule itself reads any predicate (the tests run it
//! over staggered offsets); only the adapter moves. A later deferral of skipped audits (the pool's
//! "v2" TODO) moves `S` again and must move the adapter with it.

use crate::palw_readiness_escalation::palw_readiness_proof_in_flight_v1;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
use kaspa_consensus_core::palw_readiness_escalation_v1::PALW_READINESS_ESCALATION_LANDING_DAA_V1;
use kaspa_hashes::Hash64;

/// **(a) pays a juror's proof only if it landed at `S − 2` or earlier** (the Activation Pool's
/// preparation reward: the span the fold accepted the proof in, before the span whose anchor seeds
/// the jury).
pub const PALW_CANDIDATE_PROOF_PAID_BY_SPANS_V1: u64 = 2;

/// **The node reads a block's step one DAA after it**: its virtual is the block's DAA + 1, so a
/// Candidate admitted at its audit block `S` is proved on today's cadence from `S + 1` at the earliest.
pub const PALW_CANDIDATE_PROOF_VIEW_LAG_DAA_V1: u64 = 1;

/// **Is P3 in force at `daa_score`?** R-core+'s fence — the one M1's escalation reads —
/// `false` on every preset but testnet-12.
pub fn palw_candidate_proof_timing_armed_v1(params: &Params, daa_score: u64) -> bool {
    params.palw_rcore_plus_active_at(daa_score)
}

/// **A Candidate's audit period, in spans — the fold's** (`palw_admission_audit_period_spans_v2` over
/// the bundle's epoch and `Params::palw_admission_audit_period_daa`, the inputs the fold is handed).
/// A network without a V2 bundle gets one span, which no schedule applies to.
pub fn palw_candidate_audit_period_spans_v1(params: &Params, span_daa: u64) -> u64 {
    let epoch_length = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.epoch_length(),
        _ => 0,
    };
    kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_period_spans_v2(
        epoch_length,
        span_daa,
        params.palw_admission_audit_period_daa,
    )
}

/// **Is `span` an admission audit of `class_id` — the fold's own predicate, asked as the fold asks
/// it** (`admission_jury_seated`). On this branch ADR-0147's: every positive multiple of the period,
/// for every class, so `params` and `class_id` are not read yet. Past R2 the fold keys it by class;
/// the merge that brings R2 makes this
/// `if params.palw_activation_pool_fence().is_some() { palw_admission_audit_due_staggered_v1(class_id,
/// span, period) } else { palw_admission_audit_due_v1(span, period) }` (R2's fence is genesis-only),
/// and `the_node_asks_the_folds_own_audit_predicate` holds it to that.
pub fn palw_candidate_audit_due_v1(_params: &Params, _class_id: &Hash64, span: u64, period_spans: u64) -> bool {
    kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_due_v1(span, period_spans)
}

/// **Where a Candidate's proof goes, in spans before its audit.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCandidateProofWindowV1 {
    /// The last span a proof may be sent in and still land by `S − 2` at M1's carriage: `S − 4` on
    /// testnet-12.
    pub last_lead: u64,
    /// The span the proof is first sent in — as early as the admission's cut allows: `S − 5` on
    /// testnet-12.
    pub target_lead: u64,
}

/// **The window on a clock of `span_daa` DAA a span and rows of `max_age_daa`**: sent in span `s`, a
/// proof lands by span `s + ⌈landing / span⌉`, so (a)'s cut is `last_lead = 2 + ⌈landing / span⌉`;
/// the row a proof sent in span `S − k` writes stands until `(S − k)·span + max_age`, and the ordinary
/// duty that follows an admission is accepted by `S·span + lag + landing`, so the admission's cut is
/// `k ≤ (max_age − lag − landing) / span` — the target. `None` where that is before `last_lead`: a row
/// too short-lived for both cuts, and today's duty applies.
pub fn palw_candidate_proof_window_v1(span_daa: u64, max_age_daa: u64) -> Option<PalwCandidateProofWindowV1> {
    let span = span_daa.max(1);
    let landing = PALW_READINESS_ESCALATION_LANDING_DAA_V1;
    let last_lead = PALW_CANDIDATE_PROOF_PAID_BY_SPANS_V1 + landing.div_ceil(span);
    let target_lead = max_age_daa.saturating_sub(PALW_CANDIDATE_PROOF_VIEW_LAG_DAA_V1 + landing) / span;
    (target_lead >= last_lead).then_some(PalwCandidateProofWindowV1 { last_lead, target_lead })
}

/// **Does `row` serve the audit at `audit_span`?** It counts past readiness V2 (a V2 row there) and it
/// was proved in the window (`audit_span − target_lead` or later), so it stands at the audit and
/// through an admission there, and at the audit it is not lapsing. A row the tip holds has landed, so
/// in the window it landed by `S − 4`.
pub fn palw_candidate_row_serves_audit_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    readiness_v2: bool,
    audit_span: u64,
    window: &PalwCandidateProofWindowV1,
    span_daa: u64,
) -> bool {
    row.is_some_and(|row| {
        !(readiness_v2 && row.proof_version < 2)
            && row.proved_daa >= audit_span.saturating_sub(window.target_lead).saturating_mul(span_daa.max(1))
    })
}

/// The first audit span at or after `from`, within one period of it; `None` if `audit_due` names none.
pub fn palw_candidate_next_audit_span_v1(from: u64, period_spans: u64, audit_due: impl Fn(u64) -> bool) -> Option<u64> {
    (from..=from.saturating_add(period_spans.max(1))).find(|span| audit_due(*span))
}

/// Why a Candidate's proof is held this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCandidateProofHoldV1 {
    /// Before the window: the aligned proof goes at `send_from_span`. The spans `S − 3 … S` of the
    /// audit before are here too — they wait for the NEXT audit's window.
    Early { send_from_span: u64 },
    /// The row at the tip already serves the audit: proved in its window, landed by `S − 4`.
    Served,
    /// This audit's proof was submitted and may still land by `S − 2` — or, at the audit span, the
    /// hand-off was: no copy.
    Submitted,
}

/// **What this seat does about a class's proof this tick.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCandidateProofPlanV1 {
    /// Not armed, not a `Candidate`, or no window on this clock: today's duty and M1, unchanged.
    Today,
    /// Send this span's proof now, whether today's duty would or not, at the Own site (never M1's
    /// escalated one): the aligned proof for the audit at `audit_span`, or — at the audit span
    /// itself — the hand-off.
    Send { audit_span: u64, handoff: bool },
    /// No proof for the class this tick, whatever today's duty or M1 would say.
    Hold { audit_span: u64, why: PalwCandidateProofHoldV1 },
}

impl PalwCandidateProofPlanV1 {
    /// The seat's status line for a held class (`readiness_note`, logged when it changes).
    pub fn note(&self) -> String {
        match self {
            Self::Today => "today's cadence".to_string(),
            Self::Send { audit_span, handoff: false } => format!("candidate: proving for its admission audit at span {audit_span}"),
            Self::Send { audit_span, handoff: true } => {
                format!("candidate: the hand-off proof at its admission audit (span {audit_span})")
            }
            Self::Hold { audit_span, why: PalwCandidateProofHoldV1::Early { send_from_span } } => format!(
                "candidate: no proof until span {send_from_span}, for its admission audit at span {audit_span} (P3: one proof a \
                 period, landing by the audit's span − 2)"
            ),
            Self::Hold { audit_span, why: PalwCandidateProofHoldV1::Served } => {
                format!("candidate: its row serves the admission audit at span {audit_span}; no proof until then")
            }
            Self::Hold { audit_span, why: PalwCandidateProofHoldV1::Submitted } => {
                format!("candidate: its proof for the admission audit at span {audit_span} is landing; no copy")
            }
        }
    }
}

/// **P3's decision for one class this tick** — `armed` ([`palw_candidate_proof_timing_armed_v1`] at
/// `now_daa`), whether the class is in lifecycle `Candidate` at the tip, the tip's `row` for this
/// seat, the span it names now, its last submitted proof for the class, the clock (`span_daa`, the
/// registry's row age `max_age_daa`, the audit period) and the fold's audit predicate. See the
/// module's header for the schedule.
#[allow(clippy::too_many_arguments)]
pub fn palw_candidate_proof_plan_v1(
    armed: bool,
    candidate: bool,
    row: Option<&PalwSeatReadinessRowV1>,
    readiness_v2: bool,
    span_now: u64,
    last_submitted_span: Option<u64>,
    now_daa: u64,
    span_daa: u64,
    max_age_daa: u64,
    period_spans: u64,
    audit_due: impl Fn(u64) -> bool,
) -> PalwCandidateProofPlanV1 {
    if !armed || !candidate {
        return PalwCandidateProofPlanV1::Today;
    }
    let Some(window) = palw_candidate_proof_window_v1(span_daa, max_age_daa) else { return PalwCandidateProofPlanV1::Today };
    palw_candidate_proof_plan_in_window_v1(
        &window,
        row,
        readiness_v2,
        span_now,
        last_submitted_span,
        now_daa,
        span_daa,
        period_spans,
        audit_due,
    )
}

/// [`palw_candidate_proof_plan_v1`] for an armed `Candidate` in a given window (the tests run it in a
/// window the shipped clock does not give, to measure why it does not).
#[allow(clippy::too_many_arguments)]
pub fn palw_candidate_proof_plan_in_window_v1(
    window: &PalwCandidateProofWindowV1,
    row: Option<&PalwSeatReadinessRowV1>,
    readiness_v2: bool,
    span_now: u64,
    last_submitted_span: Option<u64>,
    now_daa: u64,
    span_daa: u64,
    period_spans: u64,
    audit_due: impl Fn(u64) -> bool,
) -> PalwCandidateProofPlanV1 {
    use PalwCandidateProofHoldV1::{Early, Served, Submitted};
    use PalwCandidateProofPlanV1::{Hold, Send, Today};
    // A period no longer than the window would put one audit's window inside the last one's quiet
    // spans; there the half-age cadence already proves before every audit.
    if period_spans <= window.target_lead {
        return Today;
    }
    // The hand-off: at the audit span itself, by a seat whose row serves it — nothing sent now lands
    // before the audit's block, and the row is not lapsing yet, so it is an ordinary proof.
    if audit_due(span_now) && palw_candidate_row_serves_audit_v1(row, readiness_v2, span_now, window, span_daa) {
        return if last_submitted_span == Some(span_now) {
            Hold { audit_span: span_now, why: Submitted }
        } else {
            Send { audit_span: span_now, handoff: true }
        };
    }
    // The first audit a proof sent now can still be paid at; the ones closer are the quiet spans.
    let Some(audit_span) = palw_candidate_next_audit_span_v1(span_now.saturating_add(window.last_lead), period_spans, &audit_due)
    else {
        return Today;
    };
    let send_from_span = audit_span.saturating_sub(window.target_lead);
    if span_now < send_from_span {
        return Hold { audit_span, why: Early { send_from_span } };
    }
    if palw_candidate_row_serves_audit_v1(row, readiness_v2, audit_span, window, span_daa) {
        return Hold { audit_span, why: Served };
    }
    if last_submitted_span.is_some_and(|last| last >= send_from_span)
        || palw_readiness_proof_in_flight_v1(last_submitted_span, now_daa, span_daa)
    {
        return Hold { audit_span, why: Submitted };
    }
    Send { audit_span, handoff: false }
}

/// **The Own site's order with a Candidate's proofs in it** — what `readiness_duties` hands the
/// tick: every other proof keeps its place, and the Candidate proofs (in their own order) go right
/// after the last one that is `hurried` — a counted row's proof M1 escalates, lapsing or lapsed. So a
/// Candidate's proof never goes ahead of a counted row M1 is hurrying, and a counted row's proof M1
/// does not hurry yet (it escalates if it waits a tick) does not starve a Candidate's two-span
/// window under a DA storm, whatever order the registry lists the classes in. With no Candidate proof
/// the order is today's, untouched.
pub fn palw_candidate_own_order_v1<T>(duties: Vec<T>, is_candidate: impl Fn(&T) -> bool, hurried: impl Fn(&T) -> bool) -> Vec<T> {
    let (candidates, mut others): (Vec<T>, Vec<T>) = duties.into_iter().partition(|duty| is_candidate(duty));
    let at = others.iter().rposition(hurried).map_or(0, |last| last + 1);
    others.splice(at..at, candidates);
    others
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::{PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1};
    use crate::palw_readiness_escalation::{palw_readiness_duty_urgency_v1, palw_readiness_duty_waits_v1};
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwRegistryGlobalsV1, palw_readiness_duty_due_v2, palw_readiness_max_age_daa_v1,
    };
    use kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1;

    const G: PalwRegistryGlobalsV1 = PALW_REGISTRY_GLOBALS_V1;
    /// Testnet-12's audit period in spans (`palw_candidate_audit_period_spans_v1`, pinned below).
    const PERIOD: u64 = 100;

    fn max_age() -> u64 {
        palw_readiness_max_age_daa_v1(1, &G, true)
    }

    fn row(proved_daa: u64) -> PalwSeatReadinessRowV1 {
        PalwSeatReadinessRowV1 { proved_daa, proved_span: proved_daa, leaf_index: 0, proof_version: 2, chunks: 16 }
    }

    /// R2's shape — `(span + offset) mod period == 0`, span zero never: the tests' stand-in for a
    /// class's staggered audit (offset 0 is this branch's unstaggered one).
    fn audits(offset: u64) -> impl Fn(u64) -> bool {
        move |span| span > 0 && (span % PERIOD + offset).is_multiple_of(PERIOD)
    }

    /// The counterfactuals the shipped choices are measured against.
    #[derive(Clone, Copy, Debug)]
    struct Shape {
        /// Send the aligned proof this many spans before the audit instead of the window's target.
        target_lead: Option<u64>,
        /// Send the hand-off at the audit span (shipped: yes).
        handoffs: bool,
        /// Let a Candidate's proof carry M1's urgency to the escalated site (shipped: no).
        candidate_escalates: bool,
    }

    const SHIPPED: Shape = Shape { target_lead: None, handoffs: true, candidate_escalates: false };

    /// **`readiness_duties`' decision for one class, composed exactly as the panel composes it**
    /// (`the_panel_asks_the_plan_before_todays_duty` pins the panel's text): `None`, no proof;
    /// `Some((urgency, candidate))`, a proof — at M1's escalated site when `urgency` is `Some`, and a
    /// Candidate's (`palw_candidate_own_order_v1` places it) when `candidate`.
    fn class_duty_of(
        armed: bool,
        candidate: bool,
        row: Option<&PalwSeatReadinessRowV1>,
        last: Option<u64>,
        now: u64,
        audit_due: &dyn Fn(u64) -> bool,
        shape: Shape,
    ) -> Option<(Option<PalwReadinessUrgencyV1>, bool)> {
        let urgency = palw_readiness_duty_urgency_v1(armed, row, now, 2, last, now, 1, &G, true);
        let plan = match shape.target_lead {
            Some(target_lead) if armed && candidate => palw_candidate_proof_plan_in_window_v1(
                &PalwCandidateProofWindowV1 { last_lead: 4, target_lead },
                row,
                true,
                now,
                last,
                now,
                1,
                PERIOD,
                audit_due,
            ),
            _ => palw_candidate_proof_plan_v1(armed, candidate, row, true, now, last, now, 1, max_age(), PERIOD, audit_due),
        };
        match plan {
            PalwCandidateProofPlanV1::Hold { .. } => None,
            PalwCandidateProofPlanV1::Send { handoff: true, .. } if !shape.handoffs => None,
            PalwCandidateProofPlanV1::Send { .. } => Some((if shape.candidate_escalates { urgency } else { None }, true)),
            PalwCandidateProofPlanV1::Today => (palw_readiness_duty_due_v2(row, now, now, last, 1, &G, true)
                && !palw_readiness_duty_waits_v1(armed, last, now, 1))
            .then_some((urgency, false)),
        }
    }

    /// [`class_duty_of`]'s urgency alone.
    fn class_duty(
        armed: bool,
        candidate: bool,
        row: Option<&PalwSeatReadinessRowV1>,
        last: Option<u64>,
        now: u64,
        audit_due: &dyn Fn(u64) -> bool,
        shape: Shape,
    ) -> Option<Option<PalwReadinessUrgencyV1>> {
        class_duty_of(armed, candidate, row, last, now, audit_due, shape).map(|(urgency, _)| urgency)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Kind {
        /// A `Candidate` audited at `offset`; `admitted`: the audit block that admits it, after which
        /// its rows count every span (`Prefetching`, `Probation`).
        Candidate { offset: u64, admitted: Option<u64> },
        /// A class the chain counts every span.
        Counted,
    }

    impl Kind {
        /// In `Candidate` at the node's tick `t` — the state after block `t − 1`.
        fn candidate_at(&self, t: u64) -> bool {
            matches!(self, Kind::Candidate { admitted, .. } if admitted.is_none_or(|at| t <= at))
        }
        /// Whether block `t`'s step counts the class's rows (every span, or never before admission).
        fn counted_at(&self, t: u64) -> bool {
            match self {
                Kind::Counted => true,
                Kind::Candidate { admitted, .. } => admitted.is_some_and(|at| t > at),
            }
        }
    }

    struct Sim {
        armed: bool,
        /// Blocks from the virtual DAA a proof is sent at to the chain block that accepts it (1: the
        /// block being mined carries it; 2: M1's carriage).
        delay: u64,
        /// A court queue that never empties and a licence always waiting (M1's single-seat storm).
        storm: bool,
        shape: Shape,
        /// Which sends the mempool refuses (`(tick, class)`): not submitted, the slot not taken.
        refused: fn(u64, usize) -> bool,
    }

    fn sim(armed: bool, delay: u64, storm: bool) -> Sim {
        Sim { armed, delay, storm, shape: SHIPPED, refused: never }
    }

    #[derive(Debug, Default)]
    struct Out {
        /// `(tick, class, escalated)`.
        sent: Vec<(u64, usize, bool)>,
        /// `(block, class)` for every block whose step found a counted row stale.
        lapsed: Vec<(u64, usize)>,
        /// `(audit block, class, landed span, (a) pays)` — the latest landing the step reads, and
        /// whether the row is fresh at the audit with that landing at `S − 2` or earlier.
        audits: Vec<(u64, usize, Option<u64>, bool)>,
    }

    impl Out {
        fn sent_by(&self, class: usize) -> Vec<(u64, bool)> {
            self.sent.iter().filter(|(_, c, _)| *c == class).map(|(t, _, e)| (*t, *e)).collect()
        }
    }

    /// **One seat's ticks, one DAA each, on testnet-12's clock**, through the tick's own scheduler and
    /// site order and `readiness_duties`' own composition ([`class_duty`]): a proof sent at tick `v`
    /// is accepted by block `v + delay` (its landing span, the row it writes dated at `v`) and the
    /// node sees it from tick `v + delay + 1`; block `t`'s step reads the rows accepted by `t − 1`.
    /// One carrier a tick (`MAX_INFLIGHT_CARRIERS`), carried by the next block. Every class starts
    /// with a row dated `start[c]`; the classes are read in the order given.
    fn run(sim: &Sim, kinds: &[Kind], start: &[u64], ticks: u64) -> Out {
        let classes = kinds.len();
        let mut rows: Vec<PalwSeatReadinessRowV1> = start.iter().map(|d| row(*d)).collect();
        let mut landed: Vec<Option<u64>> = vec![None; classes];
        let mut landing: Vec<(u64, usize, u64)> = Vec::new();
        let mut last: Vec<Option<u64>> = vec![None; classes];
        let mut last_lane: Option<PalwCarrierLaneV1> = None;
        let mut out = Out::default();
        let due: Vec<Box<dyn Fn(u64) -> bool>> = kinds
            .iter()
            .map(|k| match k {
                Kind::Candidate { offset, .. } => Box::new(audits(*offset)) as Box<dyn Fn(u64) -> bool>,
                Kind::Counted => Box::new(|_| false),
            })
            .collect();
        for t in 10..ticks {
            // The rows block `t − 1` accepted: the node and block `t`'s step read them now.
            landing.retain(|(at, c, span)| {
                if *at + 1 == t {
                    rows[*c] = row(*span);
                    landed[*c] = Some(*at);
                }
                *at + 1 != t
            });
            // Block t's step.
            for c in 0..classes {
                let fresh = t.saturating_sub(rows[c].proved_daa) <= max_age();
                if kinds[c].counted_at(t) && !fresh {
                    out.lapsed.push((t, c));
                }
                if matches!(kinds[c], Kind::Candidate { .. }) && !kinds[c].counted_at(t) && due[c](t) {
                    let paid = fresh && landed[c].is_some_and(|l| l + PALW_CANDIDATE_PROOF_PAID_BY_SPANS_V1 <= t);
                    out.audits.push((t, c, landed[c], paid));
                }
            }
            // Tick t: `readiness_duties`, then the sites.
            let duties: Vec<(usize, Option<PalwReadinessUrgencyV1>, bool)> = (0..classes)
                .filter_map(|c| {
                    class_duty_of(sim.armed, kinds[c].candidate_at(t), Some(&rows[c]), last[c], t, &*due[c], sim.shape)
                        .map(|(urgency, candidate)| (c, urgency, candidate))
                })
                .collect();
            let mut duties = palw_candidate_own_order_v1(duties, |(_, _, candidate)| *candidate, |(_, urgency, _)| urgency.is_some());
            let mut slots = PalwCarrierSlotsV1::new(last_lane);
            let mut inflight = 0usize; // last tick's carrier was carried by this block
            for site in PalwCarrierSiteV1::TICK_ORDER {
                slots.at(site, inflight);
                if !slots.offers(site, inflight) {
                    continue;
                }
                let proof = match site {
                    PalwCarrierSiteV1::ReadinessEscalated => duties
                        .iter()
                        .enumerate()
                        .filter_map(|(at, (_, urgency, _))| urgency.map(|urgency| (urgency, at)))
                        .min()
                        .map(|(_, at)| (duties.remove(at).0, true)),
                    PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences | PalwCarrierSiteV1::Licences => {
                        if sim.storm {
                            inflight += 1;
                        }
                        None
                    }
                    PalwCarrierSiteV1::Own => (!duties.is_empty()).then(|| (duties.remove(0).0, false)),
                    PalwCarrierSiteV1::OwnReceipts => None,
                };
                if let Some((c, escalated)) = proof {
                    if (sim.refused)(t, c) {
                        continue;
                    }
                    last[c] = Some(t);
                    landing.push((t + sim.delay, c, t));
                    out.sent.push((t, c, escalated));
                    inflight += 1;
                }
            }
            last_lane = slots.finish(inflight);
        }
        out
    }

    fn never(_: u64, _: usize) -> bool {
        false
    }

    const OFFSETS: [u64; 8] = [0, 1, 3, 6, 7, 42, 93, 99];

    /// The audit spans of `offset` in `[from, to)`.
    fn audit_spans(offset: u64, from: u64, to: u64) -> Vec<u64> {
        (from..to).filter(|s| audits(offset)(*s)).collect()
    }

    /// **The window on testnet-12's clock is `[S − 5, S − 4]`** — `S − 4` is the last span whose proof
    /// lands by `S − 2` at M1's two-DAA carriage, and `S − 5` the earliest whose row still stands when
    /// the ordinary duty that follows an admission at `S` lands. On five-DAA spans it is `[S − 7, S − 3]`,
    /// and a row too short-lived for both cuts has none (today's duty).
    #[test]
    fn the_window_on_testnet_12_is_five_to_four_spans_before_the_audit() {
        assert_eq!(PALW_READINESS_ESCALATION_LANDING_DAA_V1, 2, "M1's carriage, which the window is built on");
        assert_eq!(max_age(), 8, "testnet-12's V2 row: eight one-DAA spans");
        assert_eq!(palw_candidate_proof_window_v1(1, max_age()), Some(PalwCandidateProofWindowV1 { last_lead: 4, target_lead: 5 }));
        assert_eq!(palw_candidate_proof_window_v1(5, 40), Some(PalwCandidateProofWindowV1 { last_lead: 3, target_lead: 7 }));
        assert_eq!(palw_candidate_proof_window_v1(1, 7), Some(PalwCandidateProofWindowV1 { last_lead: 4, target_lead: 4 }));
        assert_eq!(palw_candidate_proof_window_v1(1, 6), None, "no row dated S − 4 stands through S + 3");
        assert_eq!(palw_candidate_proof_window_v1(1, 0), None, "no registry fold: no row age, today's duty");
        // The shipped network's inputs, through the node's own readers.
        let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        assert_eq!(palw_candidate_audit_period_spans_v1(&t12, 1), PERIOD, "ADR-0147's 100-DAA standard in one-DAA spans");
        assert!(palw_candidate_proof_timing_armed_v1(&t12, 0), "testnet-12 from its first block");
        assert!(palw_candidate_audit_due_v1(&t12, &Hash64::from_u64_word(7), 300, PERIOD));
        assert!(!palw_candidate_audit_due_v1(&t12, &Hash64::from_u64_word(7), 0, PERIOD), "span zero never");
    }

    /// **The Own order**: with no Candidate proof the duties are today's, in today's order; a
    /// Candidate's proof goes behind every counted proof that is hurried (M1's urgency) and ahead of
    /// the unhurried ones after the last of those, and every other proof keeps its relative place.
    #[test]
    fn a_candidates_proof_goes_after_the_hurried_and_before_the_rest() {
        // (name, candidate, hurried)
        let order = |duties: Vec<(&'static str, bool, bool)>| -> Vec<&'static str> {
            palw_candidate_own_order_v1(duties, |d| d.1, |d| d.2).into_iter().map(|d| d.0).collect()
        };
        let counted = vec![("c1", false, false), ("c2", false, true), ("c3", false, false)];
        assert_eq!(order(counted.clone()), vec!["c1", "c2", "c3"], "no Candidate: today's order, untouched");
        assert_eq!(order(vec![("c1", false, false), ("a", true, false)]), vec!["a", "c1"], "ahead of an unhurried counted proof");
        assert_eq!(order(vec![("c1", false, true), ("a", true, false)]), vec!["c1", "a"], "never ahead of a lapsing one");
        assert_eq!(
            order(vec![("c1", false, true), ("c2", false, false), ("a", true, false), ("c3", false, true), ("b", true, false)]),
            vec!["c1", "c2", "c3", "a", "b"],
            "behind every hurried proof; the counted proofs keep their order"
        );
        assert_eq!(
            order(vec![("c1", false, true), ("a", true, false), ("c2", false, false), ("c3", false, false)]),
            vec!["c1", "a", "c2", "c3"],
            "ahead of the unhurried after the last hurried one"
        );
        assert_eq!(order(vec![("a", true, false)]), vec!["a"]);
        assert_eq!(order(Vec::new()), Vec::<&str>::new());
    }

    /// **The twin: every network but testnet-12 proves exactly as before** — P3 is never armed there,
    /// and an unarmed or non-Candidate plan is `Today` for every input, so the panel's duty and M1's
    /// urgency are today's, line for line.
    #[test]
    fn p3_is_armed_on_testnet_12_alone_and_is_today_everywhere_else() {
        use kaspa_consensus_core::config::params::{devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params};
        for (name, p) in
            [("mainnet", mainnet_shipped_params()), ("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
        {
            assert!(
                !palw_candidate_proof_timing_armed_v1(&p, 0) && !palw_candidate_proof_timing_armed_v1(&p, u64::MAX),
                "{name}: never armed"
            );
        }
        for offset in OFFSETS {
            for now in 90..=210u64 {
                for proved in [None, Some(now.saturating_sub(9)), Some(now.saturating_sub(6)), Some(now.saturating_sub(1))] {
                    let r = proved.map(row);
                    for last in [None, Some(now.saturating_sub(1)), Some(now.saturating_sub(5))] {
                        for (armed, candidate) in [(false, true), (false, false), (true, false)] {
                            let plan = palw_candidate_proof_plan_v1(
                                armed,
                                candidate,
                                r.as_ref(),
                                true,
                                now,
                                last,
                                now,
                                1,
                                max_age(),
                                PERIOD,
                                audits(offset),
                            );
                            assert_eq!(plan, PalwCandidateProofPlanV1::Today, "armed {armed}, candidate {candidate}");
                            let today = (palw_readiness_duty_due_v2(r.as_ref(), now, now, last, 1, &G, true)
                                && !palw_readiness_duty_waits_v1(armed, last, now, 1))
                            .then(|| palw_readiness_duty_urgency_v1(armed, r.as_ref(), now, 2, last, now, 1, &G, true));
                            assert_eq!(class_duty(armed, candidate, r.as_ref(), last, now, &audits(offset), SHIPPED), today);
                        }
                    }
                }
            }
        }
    }

    /// **The schedule over several audit offsets**: a Candidate's seat sends its aligned proof at
    /// `S − 5` and its hand-off at `S`, whatever the offset, and nothing else — in particular nothing in
    /// `S − 3 … S − 1`. At carriages of one to three blocks the aligned proof lands in `[S − 4, S − 2]`
    /// ⊂ `[S − 7, S − 2]` and (a) pays at every audit; a fourth block of queueing spends the slack and
    /// it lands at `S − 1`, as a quarter of today's did. Today's cadence, measured on the same clock,
    /// spends twenty to twenty-nine proofs a period on a row, and leaves a third of the audits unpaid.
    #[test]
    fn the_schedule_lands_one_proof_by_s_minus_2_and_sends_nothing_at_s_minus_1_for_any_offset() {
        const TICKS: u64 = 1_000;
        for offset in OFFSETS {
            let kinds = [Kind::Candidate { offset, admitted: None }];
            let audited = audit_spans(offset, 20, TICKS);
            let mut expected: Vec<u64> =
                audit_spans(offset, 15, TICKS + 5).iter().flat_map(|s| [s - 5, *s]).filter(|t| *t < TICKS).collect();
            expected.sort();
            for delay in 1..=4u64 {
                let out = run(&sim(true, delay, false), &kinds, &[0], TICKS);
                let sent: Vec<u64> = out.sent.iter().map(|(t, _, _)| *t).collect();
                assert_eq!(sent, expected, "offset {offset}, carriage {delay}: S − 5 and S, and nothing else");
                for (s, _, landed, paid) in out.audits.iter().filter(|(s, ..)| audited.contains(s)) {
                    let landed = landed.expect("a proof landed");
                    assert_eq!(landed, s - 5 + delay, "offset {offset}, carriage {delay}: S = {s}");
                    assert_eq!((s - 7..=s - 2).contains(&landed), delay <= 3, "offset {offset}, carriage {delay}: {landed}");
                    assert_eq!(*paid, delay <= 3, "offset {offset}, carriage {delay}: (a) at S = {s}");
                }
                assert_eq!(out.audits.len(), audit_spans(offset, 10, TICKS).len(), "every audit was read");
                assert!(out.sent.iter().all(|(.., escalated)| !escalated), "a Candidate's proof rides the Own site");
            }
            let today = run(&sim(false, 2, false), &kinds, &[0], TICKS);
            let unpaid = today.audits.iter().filter(|(.., paid)| !paid).count();
            println!("offset {offset}: unarmed, {} proofs and {unpaid} of {} audits unpaid", today.sent.len(), today.audits.len());
            assert!(unpaid > 0, "offset {offset}: today's cadence loses (a) at some audits — the hole this closes");
        }
        // What a row on today's cadence costs a period, past M1 (its copy guard armed).
        for delay in [1u64, 2] {
            let counted = run(&sim(true, delay, false), &[Kind::Counted], &[7], TICKS);
            let per_period = counted.sent.len() as f64 * PERIOD as f64 / (TICKS - 10) as f64;
            println!("today's cadence past M1, carriage {delay}: {per_period:.1} proofs a period");
            assert!(per_period >= 16.0, "{per_period}");
        }
    }

    /// **A refused proof is retried inside the window, and a submitted one is never copied**: refused
    /// at `S − 5`, the proof goes at `S − 4`, still lands by `S − 2` at M1's carriage, and the hand-off
    /// follows at `S`; refused at both, nothing goes before the audit and no hand-off after it (the
    /// audit is lost for this juror, never another proof's (a) at `S − 1`).
    #[test]
    fn a_refused_proof_is_retried_at_s_minus_4_and_never_later() {
        let kinds = [Kind::Candidate { offset: 0, admitted: None }];
        let refused_once = |t: u64, _| t % PERIOD == 95;
        let out = run(&Sim { refused: refused_once, ..sim(true, 2, false) }, &kinds, &[0], 500);
        for s in audit_spans(0, 100, 500) {
            let sent: Vec<u64> = out.sent.iter().map(|(t, ..)| *t).filter(|t| (s - 7..=s).contains(t)).collect();
            assert_eq!(sent, vec![s - 4, s], "S = {s}: the retry at S − 4, then the hand-off");
        }
        assert!(out.audits.iter().filter(|(s, ..)| *s >= 100).all(|(.., paid)| *paid), "{:?}", out.audits);
        let refused_twice = |t: u64, _| matches!(t % PERIOD, 95 | 96);
        let out = run(&Sim { refused: refused_twice, ..sim(true, 2, false) }, &kinds, &[0], 500);
        for s in audit_spans(0, 100, 500) {
            assert!(!out.sent.iter().any(|(t, ..)| (s - 7..=s).contains(t)), "S = {s}: nothing after S − 4");
        }
    }

    /// **Which wins, M1 or P3: P3, for a Candidate** — whether AND where. Under M1's single-seat storm
    /// (a court queue that never empties, a licence always waiting) with a Candidate beside a counted
    /// class, in either reading order, at every phase of the counted row and at both carriages: the
    /// counted row never lapses, and nothing of the Candidate's goes outside `S − 5`, `S − 4` and `S`
    /// or to the escalated site. The storm leaves the Own site every other slot, so the aligned proof's
    /// two spans can both fall to the court (printed: the share of audits (a) pays). The
    /// counterfactual — a Candidate's proof carrying M1's urgency to the escalated site, as its lapsed
    /// row's would — takes that slot the tick before a counted row needs it at some phase (M1 never
    /// escalates twice running) and lapses the COUNTED row. So a Candidate row about to lapse does not
    /// escalate.
    #[test]
    fn a_candidate_never_escalates_so_a_counted_row_never_lapses_under_the_storm() {
        const TICKS: u64 = 600;
        let mut counterfactual_lapses = Vec::new();
        for delay in [1u64, 2] {
            let (mut audits_read, mut paid) = (0usize, 0usize);
            for offset in [0u64, 37] {
                for phase in 0..8u64 {
                    for (kinds, start, candidate) in [
                        ([Kind::Candidate { offset, admitted: None }, Kind::Counted], [0u64, 3 + phase], 0usize),
                        ([Kind::Counted, Kind::Candidate { offset, admitted: None }], [3 + phase, 0], 1),
                    ] {
                        let out = run(&sim(true, delay, true), &kinds, &start, TICKS);
                        assert!(
                            out.lapsed.iter().all(|(t, _)| *t < 20),
                            "carriage {delay}, offset {offset}, phase {phase}, candidate read {candidate}: {:?}",
                            out.lapsed
                        );
                        let audited = audit_spans(offset, 20, TICKS);
                        for s in &audited {
                            let sent: Vec<u64> =
                                out.sent_by(candidate).iter().map(|(t, _)| *t).filter(|t| (s - 7..=*s).contains(t)).collect();
                            assert!(sent.iter().all(|t| [s - 5, s - 4, *s].contains(t)), "carriage {delay}, S = {s}: {sent:?}");
                            assert!(sent.iter().filter(|t| **t < *s).count() <= 1, "carriage {delay}, S = {s}: one aligned proof");
                        }
                        audits_read += audited.len();
                        paid += out.audits.iter().filter(|(s, _, _, p)| audited.contains(s) && *p).count();
                        assert!(out.sent_by(candidate).iter().all(|(_, escalated)| !escalated), "never the escalated site");
                        let escalating = Sim { shape: Shape { candidate_escalates: true, ..SHIPPED }, ..sim(true, delay, true) };
                        let counter = run(&escalating, &kinds, &start, TICKS);
                        counterfactual_lapses
                            .extend(counter.lapsed.iter().filter(|(t, c)| *t >= 20 && *c != candidate).map(|l| (delay, phase, *l)));
                    }
                }
            }
            println!("storm, carriage {delay}: (a) paid at {paid} of {audits_read} audits");
            assert!(paid * 2 >= audits_read, "carriage {delay}: {paid} of {audits_read}");
        }
        println!(
            "a Candidate escalating under the storm lapses a counted row at (carriage, phase, (block, class)) \
             {counterfactual_lapses:?}"
        );
        assert!(!counterfactual_lapses.is_empty(), "the counterfactual lapses a counted row");
        // The decision, on one input: a Candidate row about to lapse outside its window gets no proof at
        // all, and inside its window a proof at the Own site; a counted row's gets M1, as ever.
        let lapsing = row(100);
        let m1 = palw_readiness_duty_urgency_v1(true, Some(&lapsing), 106, 2, Some(100), 106, 1, &G, true);
        assert_eq!(m1, Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 }), "M1 would escalate it");
        assert_eq!(class_duty(true, true, Some(&lapsing), Some(100), 106, &audits(0), SHIPPED), None, "S = 200: P3 holds it");
        assert_eq!(class_duty(true, false, Some(&lapsing), Some(100), 106, &audits(0), SHIPPED), Some(m1), "a counted class: M1");
        assert!(palw_readiness_duty_urgency_v1(true, Some(&lapsing), 195, 2, Some(100), 195, 1, &G, true).is_some());
        assert_eq!(class_duty(true, true, Some(&lapsing), Some(100), 195, &audits(0), SHIPPED), Some(None), "S − 5 of S = 200");
        let prepared = row(195);
        assert!(palw_readiness_duty_urgency_v1(true, Some(&prepared), 200, 2, Some(195), 200, 1, &G, true).is_none(), "not lapsing");
        assert_eq!(class_duty(true, true, Some(&prepared), Some(195), 200, &audits(0), SHIPPED), Some(None), "the hand-off");
    }

    /// **An admitted class's rows keep counting** (staleness never lapses the row of a class that is
    /// not `Candidate`): admitted at its audit, the class's rows count every span from the next block,
    /// and today's duty takes a prepared seat's row over at the first tick that reads the class out of
    /// `Candidate`. At every phase of a counted class beside it, no row lapses at either carriage
    /// without a storm, nor under the storm at the one-block carriage (M1's own storm clock) — save a
    /// seat the storm kept from proving for the audit at all (it held no row the audit counted, and
    /// proves on today's cadence from `S + 1`; printed). At the two-block carriage one seat's two
    /// counted classes lapse under the storm with or without P3 (M1's capacity, its review's MEDIUM 3).
    /// The counterfactuals: without the hand-off the admitted row has one chance at M1's carriage, and a
    /// counted class's proof in that tick lapses it at some phase; with the aligned proof a span
    /// earlier (`S − 6`) and no hand-off it lapses at `S + 3` at every phase.
    #[test]
    fn an_admitted_class_never_lapses_across_its_admission() {
        const TICKS: u64 = 500;
        let (mut without_handoff, mut unprepared) = (Vec::new(), Vec::new());
        for (storm, delay) in [(false, 1u64), (false, 2), (true, 1)] {
            for offset in [0u64, 58] {
                for phase in 0..8u64 {
                    let admitted = audit_spans(offset, 300, TICKS)[0];
                    let kinds = [Kind::Candidate { offset, admitted: Some(admitted) }, Kind::Counted];
                    let start = [0, 3 + phase];
                    let out = run(&sim(true, delay, storm), &kinds, &start, TICKS);
                    let prepared = out.audits.iter().any(|(s, _, _, paid)| *s == admitted && *paid);
                    let lapsed: Vec<(u64, usize)> =
                        out.lapsed.iter().filter(|(t, c)| *t >= 20 && (prepared || *c == 1)).copied().collect();
                    assert!(lapsed.is_empty(), "storm {storm}, carriage {delay}, phase {phase}, admitted at {admitted}: {lapsed:?}");
                    assert!(storm || prepared, "without a storm the seat is always prepared");
                    if !prepared {
                        unprepared.push((delay, offset, phase));
                    }
                    let after: Vec<u64> = out.sent_by(0).iter().map(|(t, _)| *t).filter(|t| *t > admitted).collect();
                    assert!(after.len() as u64 >= (TICKS - admitted) / 7, "today's cadence resumed: {after:?}");
                    let no_handoff = Sim { shape: Shape { handoffs: false, ..SHIPPED }, ..sim(true, delay, storm) };
                    let no_handoff = run(&no_handoff, &kinds, &start, TICKS);
                    if prepared {
                        without_handoff.extend(no_handoff.lapsed.iter().filter(|(t, _)| *t >= 20).map(|l| (storm, delay, phase, *l)));
                    }
                    if delay == 2 {
                        let early = Shape { target_lead: Some(6), handoffs: false, ..SHIPPED };
                        let early = run(&Sim { shape: early, ..sim(true, delay, storm) }, &kinds, &start, TICKS);
                        assert!(early.lapsed.contains(&(admitted + 3, 0)), "phase {phase}: S − 6: {:?}", early.lapsed);
                    }
                }
            }
        }
        println!("without the hand-off: (storm, carriage, phase, (block, class)) {without_handoff:?}");
        println!("a seat the storm kept from its audit: (carriage, offset, phase) {unprepared:?}");
        assert!(without_handoff.iter().any(|(_, _, _, (_, c))| *c == 0), "the hand-off is the second chance");
    }

    /// **The panel reads the plan, not a copy of it**: `readiness_duties` keeps today's duty and M1's
    /// copy guard as one condition (`today_holds`, the text M1's own pin reads) and M1's urgency as it
    /// was, then asks `palw_candidate_proof_plan_v1` for every class — armed by this module's R-core+
    /// reader at the tick's DAA, the class's lifecycle `Candidate` at the tip, the registry's own row
    /// age, the fold's period and audit predicate — before it builds anything: `Hold` notes the class
    /// and builds no proof, `Send` builds one for the Own site (no urgency), `Today` is today's duty
    /// with M1's urgency. The duties go to the tick in [`palw_candidate_own_order_v1`]'s order. Every
    /// proof it builds after that is the one it always built.
    #[test]
    fn the_panel_asks_the_plan_before_todays_duty() {
        let source = include_str!("palw_panel.rs");
        let duties = &source[source.find("    fn readiness_duties(").expect("readiness_duties")..];
        let duties = &duties[..duties.find("\n    }\n").expect("its end")];
        let period = duties.find("crate::palw_candidate_proof_timing::palw_candidate_audit_period_spans_v1(").expect("the period");
        let urgency = duties.find("crate::palw_readiness_escalation::palw_readiness_duty_urgency_v1(").expect("M1's urgency");
        let today = duties
            .find("let today_holds = !kaspa_consensus_core::palw_model_registry_v1::palw_readiness_duty_due_v2(")
            .expect("today");
        let plan = duties.find("crate::palw_candidate_proof_timing::palw_candidate_proof_plan_v1(").expect("the plan");
        let build = duties.find("self.resolve_backend(session, class.class_id, class.artifact_root)").expect("the build");
        assert!(period < urgency && urgency < today && today < plan && plan < build, "period, urgency, today's duty, plan, build");
        // Compared without whitespace, so the formatter's line breaks are not the contract.
        let squeeze = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
        let arms = squeeze(&duties[plan..build]);
        for input in [
            "palw_candidate_proof_timing_armed_v1(&self.consensus_config.params, current_daa),",
            "matches!(lifecycle.state, kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Candidate)",
            "row.as_ref(), readiness_v2, span_now, last, current_daa, read.span_daa, read.readiness_max_age_daa, audit_period,",
            "palw_candidate_audit_due_v1(&self.consensus_config.params, &class.class_id, span, audit_period,)",
            "let urgency = match candidate {",
            "PalwCandidateProofPlanV1::Hold { .. } => { self.readiness_note(class.class_id, candidate.note()); continue; }",
            "PalwCandidateProofPlanV1::Send { .. } => { candidate_sends.push(class.class_id); None }",
            "PalwCandidateProofPlanV1::Today if today_holds => continue,",
            "PalwCandidateProofPlanV1::Today => urgency, };",
        ] {
            assert!(arms.contains(&squeeze(input)), "the panel reads `{input}`");
        }
        assert!(
            squeeze(&duties[today..plan]).contains(&squeeze(
                ") || crate::palw_readiness_escalation::palw_readiness_duty_waits_v1(\
                 self.consensus_config.params.palw_rcore_plus_active_at(current_daa), last, current_daa, read.span_daa, );"
            )),
            "today's duty holds on M1's copy guard, as it did"
        );
        assert!(
            squeeze(duties).ends_with(&squeeze(
                "crate::palw_candidate_proof_timing::palw_candidate_own_order_v1(out, |duty| duty.class_id().is_some_and(\
                 |class_id| candidate_sends.contains(&class_id)), |duty| duty.escalates(),)"
            )),
            "the tick gets the duties in the Own order"
        );
    }

    /// **The node asks the fold's own audit predicate** (the merge guard for R2): every
    /// `palw_admission_audit_due…` predicate the fold's admission jury asks is one
    /// [`palw_candidate_audit_due_v1`] asks, and the period is the fold's. On this branch the fold asks
    /// ADR-0147's unstaggered predicate; the merge that brings the Activation Pool line's R2 makes it
    /// ask `palw_admission_audit_due_staggered_v1` past `palw_activation_pool`, and this fails until
    /// the adapter asks it too — otherwise the node would prove for spans no jury sits at.
    #[test]
    fn the_node_asks_the_folds_own_audit_predicate() {
        let fold = include_str!("../../consensus/core/src/palw_state_v2.rs");
        let jury = &fold[fold.find("    fn admission_jury").expect("the fold's admission jury")..];
        let jury = &jury[..jury.find("\n    }\n").expect("its end")];
        let ours = include_str!("palw_candidate_proof_timing.rs");
        let adapter = &ours[ours.find("pub fn palw_candidate_audit_due_v1(").expect("the adapter")..];
        let adapter = &adapter[..adapter.find("\n}\n").expect("its end")];
        let needle = "palw_admission_audit_due";
        let mut asked: Vec<&str> = Vec::new();
        let mut rest = jury;
        while let Some(at) = rest.find(needle) {
            let name = &rest[at..];
            let end = name.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(name.len());
            asked.push(&name[..end]);
            rest = &rest[at + end..];
        }
        assert!(!asked.is_empty(), "the fold's jury asks an audit predicate");
        for predicate in asked {
            assert!(adapter.contains(&format!("{predicate}(")), "the fold asks `{predicate}` and the node does not");
        }
        assert!(jury.contains(
            "palw_admission_audit_period_spans_v2(self.params.epoch_length, fold.span_daa, fold.admission_audit_period_daa)"
        ));
        let period = &ours[ours.find("pub fn palw_candidate_audit_period_spans_v1(").expect("the period")..];
        assert!(period[..period.find("\n}\n").unwrap()].contains("params.palw_admission_audit_period_daa"));
    }
}
