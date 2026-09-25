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
//! **The window** ([`palw_candidate_proof_window_v1`]; testnet-12: one-DAA spans, eight-span rows, a
//! 100-DAA audit period) is what the cuts leave. **(a)'s**: a proof sent at `S − 4` lands by `S − 2`
//! at M1's carriage, one sent at `S − 3` at `S − 1` — so `S − 4` is the last. **The admission's**: the
//! audit may admit the class (`Candidate → Prefetching`, and with `ready_enough` on to `Probation` at
//! `S + 1`, whose step reads `panel_drawable` every span), and from then on staleness must never lapse
//! its rows. The node reads the admission at `S + 1`; its ordinary duty sent then is accepted by
//! `S + 3` at M1's carriage, which a row dated `S − 5` stands through and one dated `S − 6` does not;
//! the hand-off sent at `S` is accepted by `S + 2`, which a row dated `S − 6` stands through. So:
//!
//! * **The first send: `S − 6` or `S − 5`, by the seat's stagger** ([`palw_candidate_first_lead_v1`],
//!   a hash of bond, class and audit — about half the network's seats each). It lands at `S − 4` (the
//!   spec's target) or `S − 3` at M1's carriage. All of a seat's Candidates start at `S − 6` when
//!   more than two share the audit ([`palw_candidate_sharing_v1`]): the window's three sends then
//!   serve three. **One submitted proof per audit**: a proof that was submitted and has not landed is
//!   not copied (the copy could land at `S − 1` and override it — the loss this closes); only a proof
//!   the mempool refused, or a tick with no carrier slot, is retried, each span until `S − 4`.
//! * **`S − 3` … `S − 1`: nothing for the class** — not today's duty and not M1's escalation.
//! * **`S`: the hand-off**, only by a seat whose row serves this audit (it is prepared): one ordinary
//!   proof at the audit span itself, when nothing it carries can land before the audit's block. A row
//!   dated `S − 6` NEEDS it through an admission; for a later row it is the second chance today's
//!   cadence always has (the first being today's duty from `S + 1`).
//! * **Between audits: nothing** — nothing reads a Candidate's row there.
//!
//! So a Candidate costs a prepared seat two proofs a period and an unprepared one one, where today's
//! cadence spends twenty to twenty-nine on a row (measured below on testnet-12's clock).
//!
//! **The Own site** ([`palw_candidate_own_order_v1`]): every Candidate proof goes behind every proof
//! of a class that is not `Candidate`, hurried by M1 or not (the review of the first version, HIGH:
//! putting it ahead of a counted proof M1 did not hurry yet lapsed a counted row on a seat with two
//! counted classes — measured below, and now none). The one exception is a hand-off its row needs,
//! placed behind every proof M1 hurries: once a period, for the row of a class the audit may admit,
//! against a counted row that can still escalate the next tick. Among a seat's Candidate proofs a last
//! chance goes first, then a per-audit rotation ([`PalwCandidateProofRankV1`]), so Candidates sharing
//! a window take turns (the review, MEDIUM: a fixed order starved the same class at every audit).
//!
//! **M1 and P3 — which wins: P3, for a Candidate, whether AND where.** The spec asked that "a
//! Candidate row about to lapse still escalates"; this does the opposite, and the operator should
//! confirm it. A Candidate's row between audits is lapsed by design, so escalating it "when about to
//! lapse" would escalate it in every span of its window, and M1's escalated site — one proof a tick,
//! never two slots running — is the guarantee that keeps a COUNTED row from lapsing under a DA storm:
//! in steady state (six hundred DAA, every phase of one or two counted classes, three offsets, both
//! carriages) a Candidate taking it lapses counted rows the shipped seat keeps in 10 of 864 runs, at 7
//! distinct audits, while (a) would be paid more often (3,987 vs 2,749 of 4,896 audits). So a
//! Candidate's proof never escalates. **Every class that is not `Candidate` — and every class on a
//! network without R-core+ — gets [`PalwCandidateProofPlanV1::Today`]: today's duty and M1's
//! urgency exactly as they were**, and P3 lapses no counted row the node without it keeps (measured
//! below, one seat and eight). Where a miner's pool puts a proof once sent is M1's pool rule, which
//! reads the tip row and not the lifecycle — unchanged: the aligned proof renews a lapsed row, so it
//! heads a template only after every lapsing row; a hand-off of a row dated `S − 6` renews a lapsing
//! one.
//!
//! **Eight seats, one pool** (the review, MEDIUM; `eight_seats_prove_a_candidate_through_one_pool`,
//! M1's network model with this clock). The pool drains a tier first come, so what bounds the rows
//! landed by an audit is the first span any seat may send in: with `h` proofs a block and the
//! Candidate alone, about `min(8, 4h)` of eight rows are fresh at the audit and `min(8, 3h)` paid (8
//! and 6 at two a block — M1 measured two typical A16 proofs a block — against 6 and 4 with every
//! seat at `S − 5`). Every seat at `S − 6` would do as well there, but then only the `h` hand-offs
//! carried by `S + 1` keep rows through `S + 3` after an admission — undrawable steps at `h <
//! seat_count` — which the stagger's `S − 5` half does not need. What P3 still costs at scale: after
//! an admission every prepared row renews in one burst (at two a block, about two steps below
//! `seat_count` fresh rows per admission, where the staggered cadence has none; none at three); under
//! a DA storm with one head a block the Candidate's rows reach the jury at the head's rate, about four
//! in a window (P(the jury seats) ≈ 0.2 — the node without P3 is about as low at M1's shipped copy
//! guard and ≈ 1 were that guard one DAA longer), while the seats' court carriers, no longer waiting
//! behind Candidate proofs, move six times as often. A counted class beside it takes the capacity
//! first: at three a block the Candidate is then prepared on about two seats (the node without P3
//! prepares six by lapsing counted rows instead).
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

/// The domain of [`palw_candidate_stagger_v1`].
pub const PALW_CANDIDATE_STAGGER_DOMAIN_V1: &[u8] = b"misaka-palw/candidate-proof/stagger/v1";

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
/// it** (`admission_jury_v1`). Past `palw_activation_pool` (R2) the fold keys it by class — each
/// class meets its jury at its own span of the period — and below it ADR-0147's: every positive
/// multiple of the period, for every class. R2's fence is genesis-only, so the fence's presence is the
/// fold's `extras.activation_pool.is_some()` at every height; `the_node_asks_the_folds_own_audit_predicate`
/// holds this to the fold.
pub fn palw_candidate_audit_due_v1(params: &Params, class_id: &Hash64, span: u64, period_spans: u64) -> bool {
    if params.palw_activation_pool_fence().is_some() {
        kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(class_id, span, period_spans)
    } else {
        kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_due_v1(span, period_spans)
    }
}

/// **Where a Candidate's proof goes, in spans before its audit `S`.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCandidateProofWindowV1 {
    /// The last span a proof may be sent in and still land by `S − 2` at M1's carriage: `S − 4` on
    /// testnet-12. Nothing is sent for the class after it until the audit span.
    pub last_lead: u64,
    /// The latest send whose row stands through an admission at `S` on today's duty alone (read at
    /// `S + 1`, landing at M1's carriage): `S − 5` on testnet-12. A row dated before it needs the
    /// hand-off through an admission.
    pub target_lead: u64,
    /// The earliest send whose row the hand-off (sent at `S`, landing at M1's carriage) carries
    /// through an admission: `S − 6` on testnet-12, landing at `S − 4` there (the spec's target) and
    /// leaving two retries before `last_lead`. About half the seats send first here.
    pub early_lead: u64,
}

/// **The window on a clock of `span_daa` DAA a span and rows of `max_age_daa`**: sent in span `s`, a
/// proof lands by span `s + ⌈landing / span⌉`, so (a)'s cut is `last_lead = 2 + ⌈landing / span⌉`.
/// The row a proof sent in span `S − k` writes stands until `(S − k)·span + max_age`. The ordinary
/// duty that follows an admission is accepted by `S·span + lag + landing`, so without the hand-off
/// `k ≤ (max_age − lag − landing) / span` — `target_lead`; the hand-off is accepted by
/// `S·span + landing`, so with it `k ≤ (max_age − landing) / span` — `early_lead`. `None` where
/// `target_lead` is before `last_lead`: a row too short-lived for both cuts, and today's duty applies.
pub fn palw_candidate_proof_window_v1(span_daa: u64, max_age_daa: u64) -> Option<PalwCandidateProofWindowV1> {
    let span = span_daa.max(1);
    let landing = PALW_READINESS_ESCALATION_LANDING_DAA_V1;
    let last_lead = PALW_CANDIDATE_PROOF_PAID_BY_SPANS_V1 + landing.div_ceil(span);
    let target_lead = max_age_daa.saturating_sub(PALW_CANDIDATE_PROOF_VIEW_LAG_DAA_V1 + landing) / span;
    let early_lead = max_age_daa.saturating_sub(landing) / span;
    (target_lead >= last_lead).then_some(PalwCandidateProofWindowV1 { last_lead, target_lead, early_lead })
}

/// **This seat's stagger for one class at one audit** — a hash of (bond, class, audit span) that
/// picks the span this seat first sends in ([`palw_candidate_first_lead_v1`]: `S − 6` or `S − 5`,
/// about half the network each) and orders this seat's Candidate proofs ([`PalwCandidateProofRankV1`]),
/// so which of its Candidates sharing a window waits turns from one audit to the next instead of
/// being the class the registry lists last every time. Node policy: fixed so a seat's behaviour is
/// reproducible from its log.
pub fn palw_candidate_stagger_v1(bond: &[u8], class_id: &Hash64, audit_span: u64) -> u64 {
    let mut state = blake2b_simd::Params::new().hash_length(8).to_state();
    state.update(PALW_CANDIDATE_STAGGER_DOMAIN_V1);
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(&class_id.as_bytes());
    state.update(&audit_span.to_le_bytes());
    let digest = state.finalize();
    let mut word = [0u8; 8];
    word.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(word)
}

/// **The span, before its audit, a seat first sends a Candidate's proof in**: `S − early_lead` or
/// `S − target_lead` by its stagger — about half the seats each on testnet-12 (`S − 6`, `S − 5`) —
/// unless more of this seat's Candidates share the audit (`sharing`) than that spreads over: then
/// every one of them from `S − early_lead`, so the window's three sends (`S − 6 … S − 4`) serve three.
pub fn palw_candidate_first_lead_v1(window: &PalwCandidateProofWindowV1, stagger: u64, sharing: usize) -> u64 {
    let spread = window.early_lead.saturating_sub(window.target_lead).saturating_add(1);
    if sharing as u64 > spread { window.early_lead } else { window.target_lead + stagger % spread }
}

/// **How many of this seat's Candidates are audited at `audit_span`** — the class being planned and
/// every other one in `held` (the `Candidate` classes this bond has a readiness row for: the ones it
/// proves) that `audit_due` names there.
pub fn palw_candidate_sharing_v1(
    class_id: &Hash64,
    held: &[Hash64],
    audit_span: u64,
    audit_due: impl Fn(&Hash64, u64) -> bool,
) -> usize {
    1 + held.iter().filter(|id| *id != class_id && audit_due(id, audit_span)).count()
}

/// **Does `row` serve the audit at `audit_span`?** It counts past readiness V2 (a V2 row there) and it
/// was proved in the window (`audit_span − early_lead` or later), so it stands at the audit and —
/// with the hand-off — through an admission there. A row the tip holds has landed.
pub fn palw_candidate_row_serves_audit_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    readiness_v2: bool,
    audit_span: u64,
    window: &PalwCandidateProofWindowV1,
    span_daa: u64,
) -> bool {
    row.is_some_and(|row| {
        !(readiness_v2 && row.proof_version < 2)
            && row.proved_daa >= audit_span.saturating_sub(window.early_lead).saturating_mul(span_daa.max(1))
    })
}

/// The first audit span at or after `from`, within one period of it; `None` if `audit_due` names none.
pub fn palw_candidate_next_audit_span_v1(from: u64, period_spans: u64, audit_due: impl Fn(u64) -> bool) -> Option<u64> {
    (from..=from.saturating_add(period_spans.max(1))).find(|span| audit_due(*span))
}

/// Why a Candidate's proof is held this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCandidateProofHoldV1 {
    /// Before this seat's first send: the aligned proof goes at `send_from_span`. The spans
    /// `S − 3 … S` of the audit before are here too — they wait for the NEXT audit's window.
    Early { send_from_span: u64 },
    /// The row at the tip already serves the audit: proved in its window, landed.
    Served,
    /// This audit's proof was submitted and may still land by `S − 2` — or, at the audit span, the
    /// hand-off was: no copy.
    Submitted,
}

/// **Where a Candidate's proof stands at the Own site** ([`palw_candidate_own_order_v1`]). Every
/// Candidate proof goes behind every other proof, save a hand-off its row needs (`needed_handoff`:
/// dated before `S − target_lead`, so without it an admission at `S` lapses the row at M1's
/// carriage), which goes behind the proofs M1 hurries and ahead of the ones it does not hurry yet —
/// the one tick a period the row of a class the audit may admit outranks a counted row that can
/// still escalate. Among themselves: a last chance first (`slack` 0: `S − 4`, and a needed
/// hand-off), then this seat's per-audit stagger — not the spans left, which would let the earlier
/// window of two overlapping ones win every contested slot, and under a storm starve the later
/// class at every audit — so which of the Candidates sharing a window waits turns from one audit to
/// the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCandidateProofRankV1 {
    pub needed_handoff: bool,
    pub slack: u64,
    pub rotation: u64,
}

/// **What this seat does about a class's proof this tick.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCandidateProofPlanV1 {
    /// Not armed, not a `Candidate`, or no window on this clock: today's duty and M1, unchanged.
    Today,
    /// Send this span's proof now, whether today's duty would or not, at the Own site (never M1's
    /// escalated one) behind every proof of a class that is not `Candidate`: the aligned proof for the
    /// audit at `audit_span`, or — at the audit span itself — the hand-off.
    Send { audit_span: u64, handoff: bool, rank: PalwCandidateProofRankV1 },
    /// No proof for the class this tick, whatever today's duty or M1 would say.
    Hold { audit_span: u64, why: PalwCandidateProofHoldV1 },
}

impl PalwCandidateProofPlanV1 {
    /// The seat's status line for a held class (`readiness_note`, logged when it changes). It is
    /// written before the panel resolves the class's artifact, so it says what this seat does if it
    /// holds the artifact; one that does not is named when its send span comes.
    pub fn note(&self) -> String {
        match self {
            Self::Today => "today's cadence".to_string(),
            Self::Send { audit_span, handoff: false, .. } => {
                format!("candidate: proving for its admission audit at span {audit_span}")
            }
            Self::Send { audit_span, handoff: true, .. } => {
                format!("candidate: the hand-off proof at its admission audit (span {audit_span})")
            }
            Self::Hold { audit_span, why: PalwCandidateProofHoldV1::Early { send_from_span } } => format!(
                "candidate: no proof until span {send_from_span}, for its admission audit at span {audit_span} (P3: one proof a \
                 period, landing by the audit's span − 2; whether this node holds the artifact is checked then)"
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
/// registry's row age `max_age_daa`, the audit period), the fold's audit predicate, this seat's
/// stagger for the class at an audit span ([`palw_candidate_stagger_v1`]) and how many of its
/// Candidates share that audit ([`palw_candidate_sharing_v1`]). See the module's header for the
/// schedule.
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
    stagger: impl Fn(u64) -> u64,
    sharing: impl Fn(u64) -> usize,
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
        &stagger,
        |audit_span| palw_candidate_first_lead_v1(&window, stagger(audit_span), sharing(audit_span)),
    )
}

/// [`palw_candidate_proof_plan_v1`] for an armed `Candidate` in a given window, with the first send
/// `first_lead(S)` spans before the audit `S` (clamped into the window; the tests run it in windows
/// and first sends the shipped node does not use, to measure why it does not).
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
    stagger: impl Fn(u64) -> u64,
    first_lead: impl Fn(u64) -> u64,
) -> PalwCandidateProofPlanV1 {
    use PalwCandidateProofHoldV1::{Early, Served, Submitted};
    use PalwCandidateProofPlanV1::{Hold, Send, Today};
    // A period no longer than the window would put one audit's window inside the last one's quiet
    // spans; there the half-age cadence already proves before every audit.
    if period_spans <= window.early_lead {
        return Today;
    }
    // The hand-off: at the audit span itself, by a seat whose row serves it — nothing sent now lands
    // before the audit's block. A row dated before `S − target_lead` needs it through an admission,
    // so it goes ahead of the counted proofs M1 does not hurry yet; one dated later has today's duty
    // from `S + 1` too.
    if audit_due(span_now) && palw_candidate_row_serves_audit_v1(row, readiness_v2, span_now, window, span_daa) {
        if last_submitted_span == Some(span_now) {
            return Hold { audit_span: span_now, why: Submitted };
        }
        let needs = !palw_candidate_row_serves_audit_v1(
            row,
            readiness_v2,
            span_now,
            &PalwCandidateProofWindowV1 { early_lead: window.target_lead, ..*window },
            span_daa,
        );
        let rank = PalwCandidateProofRankV1 { needed_handoff: needs, slack: u64::from(!needs), rotation: stagger(span_now) };
        return Send { audit_span: span_now, handoff: true, rank };
    }
    // The first audit a proof sent now can still be paid at; the ones closer are the quiet spans.
    let Some(audit_span) = palw_candidate_next_audit_span_v1(span_now.saturating_add(window.last_lead), period_spans, &audit_due)
    else {
        return Today;
    };
    let send_from_span = audit_span.saturating_sub(first_lead(audit_span).clamp(window.last_lead, window.early_lead));
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
    let slack = audit_span.saturating_sub(window.last_lead).saturating_sub(span_now);
    Send { audit_span, handoff: false, rank: PalwCandidateProofRankV1 { needed_handoff: false, slack, rotation: stagger(audit_span) } }
}

/// **The Own site's order with a Candidate's proofs in it** — what `readiness_duties` hands the
/// tick. Every proof of a class that is not `Candidate` keeps its place, and the Candidate proofs
/// (`rank` is `Some`) go behind all of them in their rank's order (a last chance first, then the
/// rotation) — whether M1 hurries the counted
/// proof yet or not (`hurried`): the reviewed order put a Candidate's proof ahead of a counted proof
/// M1 did not hurry yet, and a seat with two counted classes then lapsed one (the Candidate took the
/// slot, the other counted row took M1's escalated site the next tick, and the deferred one could
/// not take it the tick after — never two running). The one exception is a hand-off its row needs
/// (`needed_handoff`), placed right after the last hurried proof. With no Candidate proof the order
/// is today's, untouched.
pub fn palw_candidate_own_order_v1<T>(
    duties: Vec<T>,
    rank: impl Fn(&T) -> Option<PalwCandidateProofRankV1>,
    hurried: impl Fn(&T) -> bool,
) -> Vec<T> {
    let (candidates, mut others): (Vec<T>, Vec<T>) = duties.into_iter().partition(|duty| rank(duty).is_some());
    let mut candidates: Vec<(PalwCandidateProofRankV1, T)> =
        candidates.into_iter().map(|duty| (rank(&duty).expect("partitioned on it"), duty)).collect();
    candidates.sort_by_key(|(rank, _)| (rank.slack.min(1), rank.rotation));
    let (needed, rest): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|(rank, _)| rank.needed_handoff);
    let at = others.iter().rposition(&hurried).map_or(0, |last| last + 1);
    others.splice(at..at, needed.into_iter().map(|(_, duty)| duty));
    others.extend(rest.into_iter().map(|(_, duty)| duty));
    others
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::{PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1};
    use crate::palw_readiness_escalation::{palw_readiness_duty_urgency_v1, palw_readiness_duty_waits_v1};
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwRegistryGlobalsV1, palw_readiness_duty_due_v2, palw_readiness_landing_spans_v1,
        palw_readiness_max_age_daa_v1,
    };
    use kaspa_consensus_core::palw_readiness_escalation_v1::{PalwReadinessUrgencyV1, palw_readiness_proof_urgency_v1};

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

    /// The shipped window on testnet-12's clock.
    fn shipped_window() -> PalwCandidateProofWindowV1 {
        palw_candidate_proof_window_v1(1, max_age()).expect("testnet-12 has a window")
    }

    /// Seat `s`'s bond and class `c`'s id — the stagger's inputs in the harnesses.
    fn bond(s: usize) -> Vec<u8> {
        (s as u64 + 1).to_le_bytes().to_vec()
    }

    fn class_id(c: usize) -> Hash64 {
        Hash64::from_u64_word(c as u64 + 1)
    }

    fn stagger_of(s: usize, c: usize) -> impl Fn(u64) -> u64 {
        move |audit_span| palw_candidate_stagger_v1(&bond(s), &class_id(c), audit_span)
    }

    /// Seat `s`'s first-send lead for class `c` at the audit `audit_span`, shipped, its only Candidate there.
    fn lead_of(s: usize, c: usize, audit_span: u64) -> u64 {
        palw_candidate_first_lead_v1(&shipped_window(), stagger_of(s, c)(audit_span), 1)
    }

    /// Where a seat first sends, in the counterfactuals.
    #[derive(Clone, Copy, Debug)]
    enum Lead {
        /// The shipped rule ([`palw_candidate_first_lead_v1`]).
        Shipped,
        /// Every seat `k` spans before the audit.
        Fixed(u64),
    }

    /// The node the harnesses run, and the counterfactuals the shipped choices are measured against.
    #[derive(Clone, Copy, Debug)]
    struct Shape {
        /// P3 at all (`false`: a Candidate proves on today's cadence, M1 armed — the node before P3).
        p3: bool,
        /// P3's window instead of the shipped one.
        window: Option<PalwCandidateProofWindowV1>,
        /// Send the hand-off at the audit span (shipped: yes).
        handoffs: bool,
        /// Let a Candidate's proof carry M1's urgency to the escalated site (shipped: no).
        candidate_escalates: bool,
        /// The reviewed commit's Own order: a Candidate's proof after the last proof M1 hurries and
        /// ahead of the counted ones it does not hurry yet (shipped: behind every counted proof).
        candidate_ahead: bool,
        /// Every Candidate proof behind every counted one, a needed hand-off too (shipped: a needed
        /// hand-off goes ahead of the counted proofs M1 does not hurry yet).
        all_last: bool,
        /// Measurement only: M1's copy guard held one DAA longer (the node reads a landing one DAA
        /// after the block that accepted it, so at M1's carriage its guard lets a copy through).
        m1_guard_plus: bool,
        /// Where a seat first sends.
        lead: Lead,
    }

    const SHIPPED: Shape = Shape {
        p3: true,
        window: None,
        handoffs: true,
        candidate_escalates: false,
        candidate_ahead: false,
        all_last: false,
        m1_guard_plus: false,
        lead: Lead::Shipped,
    };
    /// The node before P3: today's cadence and M1 for every class.
    const TODAY: Shape = Shape { p3: false, ..SHIPPED };
    /// The reviewed commit (`d639f234e`): every seat's first send at `S − 5`, the hand-off for a row
    /// from `S − 5` on, a Candidate's proof ahead of the counted proofs M1 does not hurry yet.
    const REVIEWED: Shape = Shape {
        window: Some(PalwCandidateProofWindowV1 { last_lead: 4, target_lead: 5, early_lead: 5 }),
        candidate_ahead: true,
        ..SHIPPED
    };
    /// The reviewed commit's timing (`S − 5` for every seat) in the shipped Own order.
    const AT_5: Shape = Shape { lead: Lead::Fixed(5), ..SHIPPED };
    /// Every seat at `S − 6`.
    const AT_6: Shape = Shape { lead: Lead::Fixed(6), ..SHIPPED };

    /// One proof `readiness_duties` hands the tick: `(class, M1's urgency, P3's rank)` — at M1's
    /// escalated site when the urgency is `Some`, and a Candidate's when the rank is `Some`.
    type Duty = (usize, Option<PalwReadinessUrgencyV1>, Option<PalwCandidateProofRankV1>);

    /// **`readiness_duties`' decision for one class, composed exactly as the panel composes it**
    /// (`the_panel_asks_the_plan_before_todays_duty` pins the panel's text): `None`, no proof;
    /// `Some((urgency, rank))`, a proof.
    #[allow(clippy::too_many_arguments)]
    fn class_duty_of(
        armed: bool,
        candidate: bool,
        row: Option<&PalwSeatReadinessRowV1>,
        last: Option<u64>,
        now: u64,
        audit_due: &dyn Fn(u64) -> bool,
        stagger: &dyn Fn(u64) -> u64,
        sharing: &dyn Fn(u64) -> usize,
        shape: Shape,
    ) -> Option<(Option<PalwReadinessUrgencyV1>, Option<PalwCandidateProofRankV1>)> {
        let guarded = shape.m1_guard_plus && last.is_some_and(|l| now == l + 2);
        let urgency = palw_readiness_duty_urgency_v1(armed, row, now, 2, last, now, 1, &G, true).filter(|_| !guarded);
        let overridden = shape.window.is_some() || !matches!(shape.lead, Lead::Shipped);
        let plan = if !shape.p3 {
            PalwCandidateProofPlanV1::Today
        } else if overridden && armed && candidate {
            let window = shape.window.unwrap_or_else(shipped_window);
            let first_lead = |audit_span: u64| match shape.lead {
                Lead::Shipped => palw_candidate_first_lead_v1(&window, stagger(audit_span), sharing(audit_span)),
                Lead::Fixed(k) => k,
            };
            palw_candidate_proof_plan_in_window_v1(&window, row, true, now, last, now, 1, PERIOD, audit_due, stagger, first_lead)
        } else {
            palw_candidate_proof_plan_v1(
                armed,
                candidate,
                row,
                true,
                now,
                last,
                now,
                1,
                max_age(),
                PERIOD,
                audit_due,
                stagger,
                sharing,
            )
        };
        match plan {
            PalwCandidateProofPlanV1::Hold { .. } => None,
            PalwCandidateProofPlanV1::Send { handoff: true, .. } if !shape.handoffs => None,
            PalwCandidateProofPlanV1::Send { rank, .. } => Some((if shape.candidate_escalates { urgency } else { None }, Some(rank))),
            PalwCandidateProofPlanV1::Today => (palw_readiness_duty_due_v2(row, now, now, last, 1, &G, true)
                && !palw_readiness_duty_waits_v1(armed, last, now, 1)
                && !guarded)
                .then_some((urgency, None)),
        }
    }

    /// [`class_duty_of`]'s urgency alone, for seat 0's class 0.
    fn class_duty(
        armed: bool,
        candidate: bool,
        row: Option<&PalwSeatReadinessRowV1>,
        last: Option<u64>,
        now: u64,
        audit_due: &dyn Fn(u64) -> bool,
        shape: Shape,
    ) -> Option<Option<PalwReadinessUrgencyV1>> {
        class_duty_of(armed, candidate, row, last, now, audit_due, &stagger_of(0, 0), &|_| 1, shape).map(|(urgency, _)| urgency)
    }

    /// The reviewed commit's Own order, kept to measure against.
    fn reviewed_own_order(duties: Vec<Duty>) -> Vec<Duty> {
        let (candidates, mut others): (Vec<Duty>, Vec<Duty>) = duties.into_iter().partition(|duty| duty.2.is_some());
        let at = others.iter().rposition(|duty| duty.1.is_some()).map_or(0, |last| last + 1);
        others.splice(at..at, candidates);
        others
    }

    fn own_order(duties: Vec<Duty>, shape: Shape) -> Vec<Duty> {
        if shape.candidate_ahead {
            reviewed_own_order(duties)
        } else if shape.all_last {
            palw_candidate_own_order_v1(
                duties,
                |duty| duty.2.map(|rank| PalwCandidateProofRankV1 { needed_handoff: false, ..rank }),
                |_| false,
            )
        } else {
            palw_candidate_own_order_v1(duties, |duty| duty.2, |duty| duty.1.is_some())
        }
    }

    /// M1's escalated site: the most urgent escalating proof, the first of a tie.
    fn escalated_pick(duties: &mut Vec<Duty>) -> Option<usize> {
        duties
            .iter()
            .enumerate()
            .filter_map(|(at, (_, urgency, _))| urgency.map(|urgency| (urgency, at)))
            .min()
            .map(|(_, at)| duties.remove(at).0)
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

    fn dues(kinds: &[Kind]) -> Vec<Box<dyn Fn(u64) -> bool>> {
        kinds
            .iter()
            .map(|k| match k {
                Kind::Candidate { offset, .. } => Box::new(audits(*offset)) as Box<dyn Fn(u64) -> bool>,
                Kind::Counted => Box::new(|_| false),
            })
            .collect()
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
        fn paid(&self, class: usize, from: u64) -> (usize, usize) {
            let read: Vec<_> = self.audits.iter().filter(|(s, c, ..)| *c == class && *s >= from).collect();
            (read.iter().filter(|(.., paid)| *paid).count(), read.len())
        }
    }

    /// **One seat's ticks, one DAA each, on testnet-12's clock**, through the tick's own scheduler and
    /// site order and `readiness_duties`' own composition ([`class_duty_of`], the Own order): a proof
    /// sent at tick `v` is accepted by block `v + delay` (its landing span, the row it writes dated at
    /// `v`) and the node sees it from tick `v + delay + 1`; block `t`'s step reads the rows accepted by
    /// `t − 1`. One carrier a tick (`MAX_INFLIGHT_CARRIERS`), carried by the next block. Every class
    /// starts with a row dated `start[c]`; the classes are read in the order given.
    fn run(sim: &Sim, kinds: &[Kind], start: &[u64], ticks: u64) -> Out {
        let classes = kinds.len();
        let mut rows: Vec<PalwSeatReadinessRowV1> = start.iter().map(|d| row(*d)).collect();
        let mut landed: Vec<Option<u64>> = vec![None; classes];
        let mut landing: Vec<(u64, usize, u64)> = Vec::new();
        let mut last: Vec<Option<u64>> = vec![None; classes];
        let mut last_lane: Option<PalwCarrierLaneV1> = None;
        let mut out = Out::default();
        let due = dues(kinds);
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
            let duties: Vec<Duty> = (0..classes)
                .filter_map(|c| {
                    let sharing = |audit: u64| (0..classes).filter(|k| kinds[*k].candidate_at(t) && due[*k](audit)).count();
                    class_duty_of(
                        sim.armed,
                        kinds[c].candidate_at(t),
                        Some(&rows[c]),
                        last[c],
                        t,
                        &*due[c],
                        &stagger_of(0, c),
                        &sharing,
                        sim.shape,
                    )
                    .map(|(urgency, rank)| (c, urgency, rank))
                })
                .collect();
            let mut duties = own_order(duties, sim.shape);
            let mut slots = PalwCarrierSlotsV1::new(last_lane);
            let mut inflight = 0usize; // last tick's carrier was carried by this block
            for site in PalwCarrierSiteV1::TICK_ORDER {
                slots.at(site, inflight);
                if !slots.offers(site, inflight) {
                    continue;
                }
                let proof = match site {
                    PalwCarrierSiteV1::ReadinessEscalated => escalated_pick(&mut duties).map(|c| (c, true)),
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

    /// The audit a tick `t` in its window proves for, at offset 0.
    fn audit_of(t: u64) -> u64 {
        (t / PERIOD + 1) * PERIOD
    }

    /// **The window on testnet-12's clock is `[S − 6, S − 4]`** — `S − 4` the last span whose proof
    /// lands by `S − 2` at M1's two-DAA carriage, `S − 5` the earliest whose row stands through an
    /// admission on today's duty alone, `S − 6` the earliest the hand-off carries through one. A seat
    /// first sends at `S − 6` or `S − 5` by its stagger, about half the seats each. On five-DAA spans
    /// it is `[S − 7, S − 3]`, and a row too short-lived for both cuts has none (today's duty).
    #[test]
    fn the_window_on_testnet_12_is_six_to_four_spans_before_the_audit() {
        assert_eq!(PALW_READINESS_ESCALATION_LANDING_DAA_V1, 2, "M1's carriage, which the window is built on");
        assert_eq!(max_age(), 8, "testnet-12's V2 row: eight one-DAA spans");
        assert_eq!(shipped_window(), PalwCandidateProofWindowV1 { last_lead: 4, target_lead: 5, early_lead: 6 });
        assert_eq!(
            palw_candidate_proof_window_v1(5, 40),
            Some(PalwCandidateProofWindowV1 { last_lead: 3, target_lead: 7, early_lead: 7 })
        );
        assert_eq!(
            palw_candidate_proof_window_v1(1, 7),
            Some(PalwCandidateProofWindowV1 { last_lead: 4, target_lead: 4, early_lead: 5 })
        );
        assert_eq!(palw_candidate_proof_window_v1(1, 6), None, "no row dated S − 4 stands through S + 3");
        assert_eq!(palw_candidate_proof_window_v1(1, 0), None, "no registry fold: no row age, today's duty");
        // The first send, over many seats and audits: both spans, about half each.
        let leads: Vec<u64> = (0..64).flat_map(|s| (1..=16u64).map(move |a| lead_of(s, 0, a * PERIOD))).collect();
        assert!(leads.iter().all(|l| [5, 6].contains(l)));
        let early = leads.iter().filter(|l| **l == 6).count();
        assert!((400..=624).contains(&early), "{early} of {} at S − 6", leads.len());
        assert_ne!(
            (1..=16u64).map(|a| lead_of(0, 0, a * PERIOD)).collect::<Vec<_>>(),
            (1..=16u64).map(|a| lead_of(0, 1, a * PERIOD)).collect::<Vec<_>>(),
            "keyed by the class too"
        );
        // The shipped network's inputs, through the node's own readers.
        let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        assert_eq!(palw_candidate_audit_period_spans_v1(&t12, 1), PERIOD, "ADR-0147's 100-DAA standard in one-DAA spans");
        assert!(palw_candidate_proof_timing_armed_v1(&t12, 0), "testnet-12 from its first block");
        // Testnet-12 arms R2 at genesis: class 7 meets its jury once a period, at its own span.
        let class = Hash64::from_u64_word(7);
        let due: Vec<u64> = (0..3 * PERIOD).filter(|s| palw_candidate_audit_due_v1(&t12, &class, *s, PERIOD)).collect();
        assert!(due.len() >= 2 && due.windows(2).all(|w| w[1] - w[0] == PERIOD), "one audit a period: {due:?}");
        assert!(
            due.iter()
                .all(|s| { kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(&class, *s, PERIOD) })
        );
        assert!(!palw_candidate_audit_due_v1(&t12, &class, 0, PERIOD), "span zero never");
    }

    /// **The Own order**: with no Candidate proof the duties are today's, in today's order; every
    /// other proof — hurried by M1 or not — keeps its place ahead of every Candidate proof, and the
    /// Candidate proofs follow by rank: the least slack first, then the per-audit rotation.
    #[test]
    fn a_candidates_proofs_go_behind_every_other_proof_in_rank_order() {
        let rank = |slack, rotation| Some(PalwCandidateProofRankV1 { needed_handoff: false, slack, rotation });
        let needed = |rotation| Some(PalwCandidateProofRankV1 { needed_handoff: true, slack: 0, rotation });
        // (name, rank, hurried)
        let order = |duties: Vec<(&'static str, Option<PalwCandidateProofRankV1>, bool)>| -> Vec<&'static str> {
            palw_candidate_own_order_v1(duties, |d| d.1, |d| d.2).into_iter().map(|d| d.0).collect()
        };
        let counted = vec![("c1", None, false), ("c2", None, true), ("c3", None, false)];
        assert_eq!(order(counted.clone()), vec!["c1", "c2", "c3"], "no Candidate: today's order, untouched");
        assert_eq!(order(vec![("a", rank(1, 0), false), ("c1", None, false)]), vec!["c1", "a"], "behind an unhurried counted proof");
        assert_eq!(order(vec![("a", rank(0, 0), false), ("c1", None, true)]), vec!["c1", "a"], "behind a hurried one");
        assert_eq!(
            order(vec![
                ("a", rank(2, 9), false),
                ("c1", None, false),
                ("b", rank(0, 5), false),
                ("c2", None, false),
                ("d", rank(1, 1), false),
                ("e", rank(2, 3), false),
            ]),
            vec!["c1", "c2", "b", "d", "e", "a"],
            "the counted proofs keep their order; the Candidates: a last chance, then by rotation"
        );
        assert_eq!(
            order(vec![
                ("c1", None, true),
                ("c2", None, false),
                ("h", needed(3), false),
                ("a", rank(0, 0), false),
                ("c3", None, true),
                ("c4", None, false)
            ]),
            vec!["c1", "c2", "c3", "h", "c4", "a"],
            "a needed hand-off: behind every hurried proof, ahead of the unhurried after them"
        );
        assert_eq!(order(vec![("h", needed(0), false), ("c1", None, false)]), vec!["h", "c1"], "…with none hurried, first");
        assert_eq!(order(vec![("a", rank(0, 0), false)]), vec!["a"]);
        assert_eq!(order(Vec::new()), Vec::<&str>::new());
        // The rank the plan gives: slack counts the spans left to send in; a hand-off its row needs
        // (dated before S − 5) has none left, one it does not has one.
        let w = shipped_window();
        let plan = |row: Option<PalwSeatReadinessRowV1>, now, last| {
            palw_candidate_proof_plan_in_window_v1(&w, row.as_ref(), true, now, last, now, 1, PERIOD, audits(0), |_| 7, |_| 6)
        };
        let rot = |needed_handoff, slack| PalwCandidateProofRankV1 { needed_handoff, slack, rotation: 7 };
        let send = |handoff, rank| PalwCandidateProofPlanV1::Send { audit_span: 200, handoff, rank };
        assert_eq!(plan(None, 194, None), send(false, rot(false, 2)));
        assert_eq!(plan(None, 196, None), send(false, rot(false, 0)));
        assert_eq!(plan(Some(row(194)), 200, None), send(true, rot(true, 0)), "a row from S − 6 needs its hand-off");
        assert_eq!(plan(Some(row(195)), 200, None), send(true, rot(false, 1)), "one from S − 5 has today's duty too");
        assert_eq!(
            plan(Some(row(193)), 200, None),
            PalwCandidateProofPlanV1::Hold { audit_span: 300, why: PalwCandidateProofHoldV1::Early { send_from_span: 294 } },
            "a row from before the window prepares nothing: no hand-off"
        );
        // The first send: by the stagger, unless more Candidates share the audit than it spreads over.
        assert_eq!((0..8).map(|x| palw_candidate_first_lead_v1(&w, x, 1)).collect::<Vec<_>>(), vec![5, 6, 5, 6, 5, 6, 5, 6]);
        assert!((0..8).all(|x| palw_candidate_first_lead_v1(&w, x, 2) == palw_candidate_first_lead_v1(&w, x, 1)));
        assert!((0..8).all(|x| palw_candidate_first_lead_v1(&w, x, 3) == 6), "three sharing: all from S − 6");
        let (a, b, c) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3));
        let due = |id: &Hash64, span: u64| if *id == c { span % PERIOD == 1 } else { span.is_multiple_of(PERIOD) };
        assert_eq!(palw_candidate_sharing_v1(&a, &[a, b, c], 200, due), 2, "c is audited a span later");
        assert_eq!(palw_candidate_sharing_v1(&c, &[a, b], 201, due), 1, "…alone there");
        assert_eq!(palw_candidate_sharing_v1(&a, &[], 200, due), 1, "a class this bond has no row for yet counts itself");
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
                                stagger_of(0, 0),
                                |_| 1,
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

    /// **The schedule over several audit offsets**: a Candidate's seat sends its aligned proof at its
    /// first send (`S − 6` or `S − 5`, by its stagger) and its hand-off at `S`, whatever the offset,
    /// and nothing else — in particular nothing in `S − 3 … S − 1`. The aligned proof lands in
    /// `[S − 5, S − 1]` ⊂ `[S − 7, S − 1]`: by `S − 2` — (a) paid — at every carriage of one to three
    /// blocks, and at four for a seat that sent at `S − 6` (landing `S − 4` at M1's carriage, the
    /// spec's target). Today's cadence, measured on the same clock, spends twenty to twenty-nine
    /// proofs a period on a row and leaves audits unpaid.
    #[test]
    fn the_schedule_lands_one_proof_by_s_minus_2_and_sends_nothing_at_s_minus_1_for_any_offset() {
        const TICKS: u64 = 1_000;
        let mut leads = [0usize; 7];
        for offset in OFFSETS {
            let kinds = [Kind::Candidate { offset, admitted: None }];
            let audited = audit_spans(offset, 20, TICKS);
            for delay in 1..=4u64 {
                let out = run(&sim(true, delay, false), &kinds, &[0], TICKS);
                let mut expected: Vec<u64> = audit_spans(offset, 16, TICKS + 7)
                    .iter()
                    .flat_map(|s| [s - lead_of(0, 0, *s), *s])
                    .filter(|t| (10..TICKS).contains(t))
                    .collect();
                expected.sort();
                let sent: Vec<u64> = out.sent.iter().map(|(t, _, _)| *t).collect();
                assert_eq!(sent, expected, "offset {offset}, carriage {delay}: the first send and S, and nothing else");
                for (s, _, landed, paid) in out.audits.iter().filter(|(s, ..)| audited.contains(s)) {
                    let lead = lead_of(0, 0, *s);
                    let landed = landed.expect("a proof landed");
                    assert_eq!(landed, s - lead + delay, "offset {offset}, carriage {delay}: S = {s}");
                    assert!((s - 7..*s).contains(&landed), "offset {offset}, carriage {delay}: {landed}");
                    assert_eq!(*paid, delay + 2 <= lead, "offset {offset}, carriage {delay}, first send S − {lead}: (a) at S = {s}");
                    if delay == 1 {
                        leads[lead as usize] += 1;
                    }
                }
                assert_eq!(out.audits.len(), audit_spans(offset, 10, TICKS).len(), "every audit was read");
                assert!(out.sent.iter().all(|(.., escalated)| !escalated), "a Candidate's proof rides the Own site");
            }
            let today = run(&Sim { shape: TODAY, ..sim(true, 2, false) }, &kinds, &[0], TICKS);
            let unpaid = today.audits.iter().filter(|(.., paid)| !paid).count();
            println!("offset {offset}: without P3, {} proofs and {unpaid} of {} audits unpaid", today.sent.len(), today.audits.len());
            assert!(unpaid > 0, "offset {offset}: today's cadence loses (a) at some audits — the hole this closes");
        }
        println!("first sends over the audits: S − 5 at {}, S − 6 at {}", leads[5], leads[6]);
        assert!(leads[5] > 0 && leads[6] > 0, "both first sends are exercised");
        // What a row on today's cadence costs a period, past M1 (its copy guard armed).
        for delay in [1u64, 2] {
            let counted = run(&sim(true, delay, false), &[Kind::Counted], &[7], TICKS);
            let per_period = counted.sent.len() as f64 * PERIOD as f64 / (TICKS - 10) as f64;
            println!("today's cadence past M1, carriage {delay}: {per_period:.1} proofs a period");
            assert!(per_period >= 16.0, "{per_period}");
        }
    }

    fn refuse_first(t: u64, _: usize) -> bool {
        t + lead_of(0, 0, audit_of(t)) == audit_of(t)
    }

    fn refuse_all_but_the_last(t: u64, _: usize) -> bool {
        let s = audit_of(t);
        (s - lead_of(0, 0, s)..s - 4).contains(&t)
    }

    fn refuse_all(t: u64, _: usize) -> bool {
        let s = audit_of(t);
        (s - lead_of(0, 0, s)..=s - 4).contains(&t)
    }

    /// **A refused proof is retried inside the window, and a submitted one is never copied**: refused
    /// at its first send, the proof goes the next span; refused up to `S − 5`, it goes at `S − 4` and
    /// still lands by `S − 2` at M1's carriage; the hand-off follows at `S`. Refused through `S − 4`,
    /// nothing goes before the audit and no hand-off after it (the audit is lost for this juror, never
    /// another proof's (a) at `S − 1`).
    #[test]
    fn a_refused_proof_is_retried_until_s_minus_4_and_never_later() {
        let kinds = [Kind::Candidate { offset: 0, admitted: None }];
        let out = run(&Sim { refused: refuse_first, ..sim(true, 2, false) }, &kinds, &[0], 1_000);
        for s in audit_spans(0, 100, 1_000) {
            let sent: Vec<u64> = out.sent.iter().map(|(t, ..)| *t).filter(|t| (s - 7..=s).contains(t)).collect();
            assert_eq!(sent, vec![s - lead_of(0, 0, s) + 1, s], "S = {s}: the retry, then the hand-off");
        }
        assert!(out.audits.iter().filter(|(s, ..)| *s >= 100).all(|(.., paid)| *paid), "{:?}", out.audits);
        let out = run(&Sim { refused: refuse_all_but_the_last, ..sim(true, 2, false) }, &kinds, &[0], 1_000);
        for s in audit_spans(0, 100, 1_000) {
            let sent: Vec<u64> = out.sent.iter().map(|(t, ..)| *t).filter(|t| (s - 7..=s).contains(t)).collect();
            assert_eq!(sent, vec![s - 4, s], "S = {s}: the last chance, then the hand-off");
        }
        assert!(out.audits.iter().filter(|(s, ..)| *s >= 100).all(|(.., paid)| *paid), "S − 4 lands at S − 2: {:?}", out.audits);
        let out = run(&Sim { refused: refuse_all, ..sim(true, 2, false) }, &kinds, &[0], 1_000);
        for s in audit_spans(0, 100, 1_000) {
            assert!(!out.sent.iter().any(|(t, ..)| (s - 7..=s).contains(t)), "S = {s}: nothing after S − 4");
        }
    }

    /// **Which wins, M1 or P3: P3, for a Candidate** — whether AND where; the spec's wording asked for
    /// the opposite ("a Candidate row about to lapse still escalates"), so this is the evidence, in
    /// steady state. Under M1's single-seat storm (a court queue that never empties, a licence always
    /// waiting), a Candidate beside one or two counted classes, in every reading order, at every
    /// phase of the counted rows, three audit offsets, both carriages, 600 DAA (five or six audits
    /// each): the shipped seat never lapses a counted row the node without P3 keeps, and nothing of
    /// the Candidate's goes outside its window or to the escalated site. The counterfactual — a
    /// Candidate's proof carrying M1's urgency to the escalated site, as its lapsed row's would —
    /// takes that site the tick before a counted row needs it (M1 never escalates twice running) and
    /// lapses COUNTED rows (printed: how many runs, and at how many distinct audits). A Candidate row
    /// between audits is lapsed by design, so "escalate when about to lapse" would also escalate it at
    /// every span of the window — the counterfactual below does exactly that.
    #[test]
    fn m1_and_p3_a_candidate_never_escalates_so_counted_rows_keep_their_guarantee() {
        const TICKS: u64 = 600;
        let escalating = Shape { candidate_escalates: true, ..SHIPPED };
        let (mut runs, mut worse_runs, mut worse_audits) = (0usize, 0usize, std::collections::BTreeSet::new());
        let (mut shipped_paid, mut escalating_paid, mut audits_read) = (0usize, 0usize, 0usize);
        for delay in [1u64, 2] {
            for offset in [0u64, 37, 58] {
                let cand = Kind::Candidate { offset, admitted: None };
                for (layout, counted) in [([0usize, 1, 9], 1usize), ([1, 0, 9], 1), ([2, 0, 1], 2), ([0, 1, 2], 2)] {
                    let classes = counted + 1;
                    let candidate = layout[0];
                    let phases: Vec<(u64, u64)> =
                        (0..8).flat_map(|p| (0..if counted == 2 { 8 } else { 1 }).map(move |q| (p, q))).collect();
                    for (p, q) in phases {
                        let mut kinds = vec![Kind::Counted; classes];
                        kinds[candidate] = cand;
                        let mut start = vec![0u64; classes];
                        let mut counted_phase = [3 + p, 3 + q].into_iter();
                        for (c, s) in start.iter_mut().enumerate() {
                            if c != candidate {
                                *s = counted_phase.next().unwrap();
                            }
                        }
                        let lapses = |shape: Shape| -> Vec<(u64, usize)> {
                            let out = run(&Sim { shape, ..sim(true, delay, true) }, &kinds, &start, TICKS);
                            out.lapsed.iter().filter(|(t, c)| *t >= 40 && *c != candidate).copied().collect()
                        };
                        let shipped_out = run(&sim(true, delay, true), &kinds, &start, TICKS);
                        let shipped = lapses(SHIPPED);
                        let today = lapses(TODAY);
                        assert!(
                            shipped.len() <= today.len(),
                            "carriage {delay}, offset {offset}, {counted} counted, candidate read {candidate}, phase {p}/{q}: \
                             P3 lapsed {shipped:?}, without P3 {today:?}"
                        );
                        if counted == 1 {
                            assert!(shipped.is_empty(), "carriage {delay}, offset {offset}, phase {p}: {shipped:?}");
                        }
                        for s in audit_spans(offset, 20, TICKS) {
                            let sent: Vec<u64> =
                                shipped_out.sent_by(candidate).iter().map(|(t, _)| *t).filter(|t| (s - 7..=s).contains(t)).collect();
                            assert!(
                                sent.iter().all(|t| (s - 6..=s - 4).contains(t) || *t == s),
                                "carriage {delay}, S = {s}: {sent:?}"
                            );
                            assert!(sent.iter().filter(|t| **t < s).count() <= 1, "carriage {delay}, S = {s}: one aligned proof");
                        }
                        assert!(shipped_out.sent_by(candidate).iter().all(|(_, escalated)| !escalated), "never the escalated site");
                        let counter = lapses(escalating);
                        let worse: Vec<&(u64, usize)> = counter.iter().filter(|l| !shipped.contains(l)).collect();
                        runs += 1;
                        if !worse.is_empty() {
                            worse_runs += 1;
                            worse_audits.extend(worse.iter().map(|(t, _)| (offset, audits_of_block(offset, *t))));
                        }
                        let from = 30;
                        shipped_paid += shipped_out.paid(candidate, from).0;
                        audits_read += shipped_out.paid(candidate, from).1;
                        let counter_out = run(&Sim { shape: escalating, ..sim(true, delay, true) }, &kinds, &start, TICKS);
                        escalating_paid += counter_out.paid(candidate, from).0;
                    }
                }
            }
        }
        println!(
            "storm: a Candidate escalating lapses counted rows the shipped seat keeps in {worse_runs} of {runs} runs, at {} \
             distinct (offset, audit) pairs; (a) paid at {shipped_paid} of {audits_read} audits shipped, {escalating_paid} escalating",
            worse_audits.len()
        );
        assert!(worse_runs > 0 && worse_audits.len() > 1, "the counterfactual lapses counted rows, at more than one audit");
        // The decision, on one input: a Candidate row about to lapse outside its window gets no proof at
        // all, and inside its window a proof at the Own site; a counted row's gets M1, as ever.
        let lapsing = row(100);
        let m1 = palw_readiness_duty_urgency_v1(true, Some(&lapsing), 106, 2, Some(100), 106, 1, &G, true);
        assert_eq!(m1, Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 }), "M1 would escalate it");
        assert_eq!(class_duty(true, true, Some(&lapsing), Some(100), 106, &audits(0), SHIPPED), None, "S = 200: P3 holds it");
        assert_eq!(class_duty(true, false, Some(&lapsing), Some(100), 106, &audits(0), SHIPPED), Some(m1), "a counted class: M1");
        let first = 200 - lead_of(0, 0, 200);
        assert!(palw_readiness_duty_urgency_v1(true, Some(&lapsing), first, 2, Some(100), first, 1, &G, true).is_some());
        assert_eq!(class_duty(true, true, Some(&lapsing), Some(100), first, &audits(0), SHIPPED), Some(None), "its first send");
        let prepared = row(first);
        assert_eq!(class_duty(true, true, Some(&prepared), Some(first), 200, &audits(0), SHIPPED), Some(None), "the hand-off");
    }

    /// The audit (of `offset`) a block `t` falls before, for grouping lapses.
    fn audits_of_block(offset: u64, t: u64) -> u64 {
        (t..).find(|s| audits(offset)(*s)).unwrap_or(t)
    }

    /// **An admitted class's rows keep counting on one seat** (staleness never lapses the row of a
    /// class that is not `Candidate`): admitted at its audit, the class's rows count every span from
    /// the next block, and today's duty takes a prepared seat's row over at the first tick that reads
    /// the class out of `Candidate`. At every phase of a counted class beside it, at three offsets,
    /// no row lapses at either carriage without a storm, nor under the storm at the one-block carriage
    /// — save a seat the storm kept from proving for the audit at all (it held no row the audit
    /// counted and proves on today's cadence from `S + 1`; printed). The counterfactuals: without the
    /// hand-off, a row first sent at `S − 6` lapses at `S + 3` at M1's carriage; and with the hand-off
    /// behind every counted proof, a counted proof due at `S` takes its slot and it lapses there too at
    /// some phase — why a hand-off its row needs goes ahead of the counted proofs M1 does not hurry yet.
    #[test]
    fn an_admitted_class_never_lapses_across_its_admission() {
        const TICKS: u64 = 500;
        let (mut without_handoff, mut unprepared, mut early_rows, mut all_last) = (Vec::new(), Vec::new(), 0usize, Vec::new());
        for (storm, delay) in [(false, 1u64), (false, 2), (true, 1)] {
            for offset in [0u64, 37, 58] {
                for phase in 0..8u64 {
                    let admitted = audit_spans(offset, 300, TICKS)[0];
                    let kinds = [Kind::Candidate { offset, admitted: Some(admitted) }, Kind::Counted];
                    let start = [0, 3 + phase];
                    let out = run(&sim(true, delay, storm), &kinds, &start, TICKS);
                    let prepared = out.audits.iter().any(|(s, _, _, paid)| *s == admitted && *paid);
                    let lapsed: Vec<(u64, usize)> =
                        out.lapsed.iter().filter(|(t, c)| *t >= 20 && (prepared || *c == 1)).copied().collect();
                    assert!(
                        lapsed.is_empty(),
                        "storm {storm}, carriage {delay}, offset {offset}, phase {phase}, admitted at {admitted}: {lapsed:?}"
                    );
                    assert!(storm || prepared, "without a storm the seat is always prepared");
                    if !prepared {
                        unprepared.push((delay, offset, phase));
                    }
                    let after: Vec<u64> = out.sent_by(0).iter().map(|(t, _)| *t).filter(|t| *t > admitted).collect();
                    assert!(after.len() as u64 >= (TICKS - admitted) / 7, "today's cadence resumed: {after:?}");
                    let no_handoff = Sim { shape: Shape { handoffs: false, ..SHIPPED }, ..sim(true, delay, storm) };
                    let no_handoff = run(&no_handoff, &kinds, &start, TICKS);
                    let early = out.sent_by(0).iter().any(|(t, _)| *t == admitted - 6);
                    if prepared && early && delay == 2 && !storm {
                        early_rows += 1;
                        assert!(
                            no_handoff.lapsed.contains(&(admitted + 3, 0)),
                            "offset {offset}, phase {phase}: a row from S − 6 without the hand-off: {:?}",
                            no_handoff.lapsed
                        );
                    }
                    if prepared {
                        without_handoff.extend(no_handoff.lapsed.iter().filter(|(t, _)| *t >= 20).map(|l| (storm, delay, phase, *l)));
                        let behind =
                            run(&Sim { shape: Shape { all_last: true, ..SHIPPED }, ..sim(true, delay, storm) }, &kinds, &start, TICKS);
                        all_last.extend(
                            behind.lapsed.iter().filter(|(t, c)| *t >= 20 && *c == 0).map(|l| (storm, delay, offset, phase, *l)),
                        );
                    }
                }
            }
        }
        println!("without the hand-off: (storm, carriage, phase, (block, class)) {without_handoff:?}");
        println!("a seat the storm kept from its audit: (carriage, offset, phase) {unprepared:?}");
        println!("the hand-off behind every counted proof: (storm, carriage, offset, phase, (block, class)) {all_last:?}");
        assert!(early_rows > 0, "some admissions read a row first sent at S − 6");
        assert!(!all_last.is_empty(), "a counted proof due at S displaces a needed hand-off somewhere");
    }

    /// **Two counted classes beside a Candidate never lapse** (the review's HIGH 1): the reviewed Own
    /// order put a Candidate's proof ahead of a counted proof M1 did not hurry yet, and a seat with
    /// two counted classes lapsed one — the Candidate took the slot, the other counted row took M1's
    /// escalated site the next tick, and the deferred one could not take it the tick after (never two
    /// running). The shipped order puts every counted proof first. Measured over the reviewer's grid:
    /// three offsets × 8 × 8 phases, both carriages, no storm — the shipped seat lapses nothing, the
    /// node without P3 nothing, the reviewed order some; (a) printed for all three.
    #[test]
    fn two_counted_classes_beside_a_candidate_never_lapse() {
        const TICKS: u64 = 600;
        for delay in [1u64, 2] {
            let (mut shipped, mut reviewed, mut today) = ((0usize, 0usize), (0usize, 0usize), (0usize, 0usize));
            let mut audits_read = 0;
            for offset in [0u64, 37, 58] {
                let kinds = [Kind::Counted, Kind::Counted, Kind::Candidate { offset, admitted: None }];
                for p in 0..8u64 {
                    for q in 0..8u64 {
                        let start = [3 + p, 3 + q, 0];
                        let measure = |shape: Shape| {
                            let out = run(&Sim { shape, ..sim(true, delay, false) }, &kinds, &start, TICKS);
                            (out.lapsed.iter().filter(|(t, c)| *t >= 30 && *c < 2).count(), out.paid(2, 30))
                        };
                        let (lapsed, (paid, read)) = measure(SHIPPED);
                        assert_eq!(lapsed, 0, "carriage {delay}, offset {offset}, phases {p}/{q}");
                        shipped = (shipped.0 + lapsed, shipped.1 + paid);
                        audits_read += read;
                        let (lapsed, (paid, _)) = measure(REVIEWED);
                        reviewed = (reviewed.0 + lapsed, reviewed.1 + paid);
                        let (lapsed, (paid, _)) = measure(TODAY);
                        today = (today.0 + lapsed, today.1 + paid);
                    }
                }
            }
            println!(
                "two counted + a Candidate, carriage {delay}, 192 runs: (counted lapses, (a) paid of {audits_read}) shipped {shipped:?}, \
                 reviewed {reviewed:?}, without P3 {today:?}"
            );
            assert_eq!(today.0, 0, "carriage {delay}: the node without P3 lapses nothing either");
            if delay == 2 {
                assert!(reviewed.0 > 0, "the reviewed order lapses counted rows at M1's carriage");
            }
        }
    }

    /// **Candidates sharing a window take turns** (the review's MEDIUM 1): two, three or four
    /// Candidates on one seat audited at the same span (this branch, where every Candidate shares
    /// `S`) or at adjacent ones, with and without the storm, at both carriages. The window gives a
    /// seat three sends (`S − 6 … S − 4`; more than two sharing an audit all start at `S − 6`), all
    /// used, and the rank rotates which class waits from one audit to the next: with up to three
    /// sharing, every class is prepared at every audit without a storm; with four, three a audit in
    /// turn; under the single-seat storm, whose court takes every other slot, one a audit in turn —
    /// never the same class starved. The reviewed order — fixed by the registry's order, two sends
    /// from `S − 5` — never prepared the third class of three.
    #[test]
    fn candidates_sharing_a_window_take_turns() {
        const TICKS: u64 = 2_000;
        let cand = |offset| Kind::Candidate { offset, admitted: None };
        for (name, kinds) in [
            ("two at one span", vec![cand(0), cand(0)]),
            ("three at one span", vec![cand(0), cand(0), cand(0)]),
            ("four at one span", vec![cand(0), cand(0), cand(0), cand(0)]),
            ("two adjacent", vec![cand(0), cand(1)]),
            ("three adjacent", vec![cand(0), cand(1), cand(2)]),
        ] {
            let start = vec![0u64; kinds.len()];
            for storm in [false, true] {
                for delay in [1u64, 2] {
                    let out = run(&sim(true, delay, storm), &kinds, &start, TICKS);
                    let paid: Vec<(usize, usize)> = (0..kinds.len()).map(|c| out.paid(c, 100)).collect();
                    let reviewed = run(&Sim { shape: REVIEWED, ..sim(true, delay, storm) }, &kinds, &start, TICKS);
                    let reviewed: Vec<(usize, usize)> = (0..kinds.len()).map(|c| reviewed.paid(c, 100)).collect();
                    println!("{name}, storm {storm}, carriage {delay}: (a) paid per class {paid:?}; reviewed {reviewed:?}");
                    let (total, audits_read) = (paid.iter().map(|(p, _)| p).sum::<usize>(), paid[0].1);
                    if !storm {
                        assert_eq!(
                            total,
                            kinds.len().min(3) * audits_read,
                            "{name}, carriage {delay}: the window's three sends, used"
                        );
                        if kinds.len() <= 3 {
                            assert!(paid.iter().all(|(p, n)| p == n), "{name}, carriage {delay}: every class at every audit");
                        }
                    } else {
                        assert!(total >= audits_read, "{name}, carriage {delay}: the storm leaves one send a window, used");
                    }
                    assert!(paid.iter().all(|(p, _)| *p > 0), "{name}, storm {storm}, carriage {delay}: no class starved");
                    if name == "three at one span" && !storm {
                        assert_eq!(reviewed[2].0, 0, "the reviewed order starved the third class");
                    }
                }
            }
        }
    }

    // ---- The network: eight seats, one pool (the review's MEDIUM 2) ----

    /// What a block carries of the readiness proofs, as M1's network model (`palw_readiness_escalation`).
    #[derive(Clone, Copy, Debug)]
    enum Lane {
        /// No other traffic: `n` proofs a block in the pool's head order.
        Open(usize),
        /// M1's DA storm under its shipped rule: every seat's court queue never empties, a licence
        /// always waits, better-paying traffic fills the fee market, so a proof lands only as the head
        /// — one whose row at the tip is lapsing or lapsed — with `beside` court carriers beside it
        /// (with none beside, the lead alternates).
        Storm { beside: usize },
    }

    /// M1's model: the ML-DSA-87 carriers that fill the lane alone.
    const H1_LANE: usize = 5;

    #[derive(Debug, Default)]
    struct Net {
        /// `(audit span, class, fresh rows, paid rows, fresh rows among the jury's population)`.
        audits: Vec<(u64, usize, usize, usize, usize)>,
        counted_rows: u64,
        /// Row-DAA of a counted class that did not count.
        counted_lapsed: u64,
        /// Row-DAA of an admitted Candidate that did not count, from its admission on.
        admitted_lapsed: u64,
        /// Steps from the admission on that found fewer than `seat_count` of its rows fresh (a class
        /// in `Probation` there is `Held`).
        admitted_undrawable: u64,
        candidate_proofs: u64,
        counted_proofs: u64,
        court: u64,
        licences: u64,
    }

    /// **P(an admission jury seats)**: `G.seat_count` operators drawn from `population` (the seats that
    /// are not the registrant's), a majority of them ready, with `ready` of the population ready.
    fn quorum_odds(ready: usize, population: usize) -> f64 {
        fn choose(n: usize, k: usize) -> f64 {
            if k > n { 0.0 } else { (0..k).fold(1.0, |acc, i| acc * (n - i) as f64 / (i + 1) as f64) }
        }
        let n = G.seat_count as usize;
        let quorum = kaspa_consensus_core::palw_model_registry_v1::palw_admission_jury_quorum_v1(G.seat_count) as usize;
        (quorum..=n).map(|x| choose(ready, x) * choose(population - ready, n - x)).sum::<f64>() / choose(population, n)
    }

    impl Net {
        /// `(mean fresh rows, mean paid rows, mean P(the jury seats))` over the audits from `from`.
        fn at_audits(&self, from: u64, seats: usize) -> (f64, f64, f64) {
            let read: Vec<_> = self.audits.iter().filter(|(s, ..)| *s >= from).collect();
            let n = read.len().max(1) as f64;
            (
                read.iter().map(|a| a.2 as f64).sum::<f64>() / n,
                read.iter().map(|a| a.3 as f64).sum::<f64>() / n,
                read.iter().map(|a| quorum_odds(a.4, seats - 1)).sum::<f64>() / n,
            )
        }
    }

    /// **The network's seats, one DAA at a time, on testnet-12's clock** — M1's network model
    /// (`palw_readiness_escalation`'s `network`: one carrier in flight per seat, held until a block
    /// carries it; a block carries the pool's head in its order — the tip row's urgency, then arrival
    /// — then court carriers; a proof past the fold's landing window is evicted) with P3's clock: a
    /// proof sent at tick `v` is carried by block `v + 1` at the earliest (M1's carriage) and accepted
    /// by the block after, whose acceptance the step and every seat read a block later. Every seat
    /// holds every class; each seat's rows of a counted class start staggered as M1's do, a
    /// Candidate's lapsed. Seat 0 is the registrant: the jury's population is the other seats.
    fn network(seats: usize, kinds: &[Kind], ticks: u64, shape: Shape, lane: Lane) -> Net {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Carrier {
            Court { sent: u64 },
            Licence,
            Proof { class: usize, span: u64, sent: u64 },
        }
        const T0: u64 = 10;
        const WARM: u64 = 30;
        let storm = matches!(lane, Lane::Storm { .. });
        let classes = kinds.len();
        let landing_window = palw_readiness_landing_spans_v1(1);
        let mut rows: Vec<Vec<PalwSeatReadinessRowV1>> = (0..seats)
            .map(|s| {
                (0..classes)
                    .map(|c| match kinds[c] {
                        Kind::Counted => row(T0 - 5 + ((s + 3 * c) % 5) as u64),
                        Kind::Candidate { .. } => row(0),
                    })
                    .collect()
            })
            .collect();
        let mut landed = vec![vec![None::<u64>; classes]; seats];
        let mut last_submitted = vec![vec![None::<u64>; classes]; seats];
        let mut last_lane = vec![None::<PalwCarrierLaneV1>; seats];
        let mut pending = vec![None::<Carrier>; seats];
        let mut landing: Vec<(u64, usize, usize, u64)> = Vec::new();
        let mut last_carried = false;
        let due = dues(kinds);
        let mut out = Net::default();
        for t in T0..T0 + ticks {
            // What block t − 1 accepted: the step, the pool and every seat read it from here.
            landing.retain(|(at, s, c, span)| {
                if *at + 1 == t {
                    rows[*s][*c] = row(*span);
                    landed[*s][*c] = Some(*at);
                }
                *at + 1 != t
            });
            // Block t's step.
            for c in 0..classes {
                let fresh: Vec<bool> = (0..seats).map(|s| t.saturating_sub(rows[s][c].proved_daa) <= max_age()).collect();
                let stale = fresh.iter().filter(|f| !**f).count();
                if t >= T0 + WARM && kinds[c].counted_at(t) {
                    if kinds[c] == Kind::Counted {
                        out.counted_rows += seats as u64;
                        out.counted_lapsed += stale as u64;
                    } else {
                        out.admitted_lapsed += stale as u64;
                        out.admitted_undrawable += u64::from(seats - stale < G.seat_count as usize);
                    }
                }
                if matches!(kinds[c], Kind::Candidate { .. }) && !kinds[c].counted_at(t) && due[c](t) {
                    let paid = (0..seats)
                        .filter(|s| fresh[*s] && landed[*s][c].is_some_and(|l| l + PALW_CANDIDATE_PROOF_PAID_BY_SPANS_V1 <= t))
                        .count();
                    let jury = (1..seats).filter(|s| fresh[*s]).count();
                    out.audits.push((t, c, seats - stale, paid, jury));
                }
            }
            // Block t: the pool's head order over the tip's rows.
            let mut heads: Vec<(Option<PalwReadinessUrgencyV1>, u64, usize)> = Vec::new();
            let mut court: Vec<(u64, usize)> = Vec::new();
            for s in 0..seats {
                match pending[s] {
                    Some(Carrier::Licence) => {
                        out.licences += 1;
                        pending[s] = None;
                    }
                    Some(Carrier::Court { sent }) => court.push((sent, s)),
                    Some(Carrier::Proof { span, .. }) if t > span + landing_window => pending[s] = None,
                    Some(Carrier::Proof { class, span, sent }) => {
                        let urgency = palw_readiness_proof_urgency_v1(Some(&rows[s][class]), span, 2, t, 1, &G, true);
                        if urgency.is_some() || !storm {
                            heads.push((urgency, sent, s));
                        }
                    }
                    None => {}
                }
            }
            heads.sort_by_key(|(urgency, sent, s)| (urgency.is_none(), *urgency, *sent, *s));
            court.sort();
            let (proofs, court_room) = match lane {
                Lane::Open(n) => (n, H1_LANE),
                Lane::Storm { beside } => {
                    let carriers_lead = beside == 0 && last_carried && !court.is_empty();
                    if heads.is_empty() || carriers_lead { (0, H1_LANE) } else { (1, beside) }
                }
            };
            last_carried = false;
            for (_, _, s) in heads.into_iter().take(proofs) {
                let Some(Carrier::Proof { class, span, .. }) = pending[s] else { unreachable!() };
                landing.push((t + 1, s, class, span));
                pending[s] = None;
                last_carried = true;
            }
            for (_, s) in court.into_iter().take(court_room) {
                pending[s] = None;
                out.court += 1;
            }
            // Tick t on every seat.
            for s in 0..seats {
                let duties: Vec<Duty> = (0..classes)
                    .filter_map(|c| {
                        class_duty_of(
                            true,
                            kinds[c].candidate_at(t),
                            Some(&rows[s][c]),
                            last_submitted[s][c],
                            t,
                            &*due[c],
                            &stagger_of(s, c),
                            &|audit: u64| (0..classes).filter(|k| kinds[*k].candidate_at(t) && due[*k](audit)).count(),
                            shape,
                        )
                        .map(|(urgency, rank)| (c, urgency, rank))
                    })
                    .collect();
                let mut duties = own_order(duties, shape);
                let mut slots = PalwCarrierSlotsV1::new(last_lane[s]);
                let mut inflight = usize::from(pending[s].is_some());
                for site in PalwCarrierSiteV1::TICK_ORDER {
                    slots.at(site, inflight);
                    if !slots.offers(site, inflight) {
                        continue;
                    }
                    let sent = match site {
                        PalwCarrierSiteV1::ReadinessEscalated => {
                            escalated_pick(&mut duties).map(|class| Carrier::Proof { class, span: t, sent: t })
                        }
                        PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences => {
                            storm.then_some(Carrier::Court { sent: t })
                        }
                        PalwCarrierSiteV1::Own => {
                            (!duties.is_empty()).then(|| Carrier::Proof { class: duties.remove(0).0, span: t, sent: t })
                        }
                        PalwCarrierSiteV1::Licences => storm.then_some(Carrier::Licence),
                        PalwCarrierSiteV1::OwnReceipts => None,
                    };
                    if let Some(carrier) = sent {
                        if let Carrier::Proof { class, .. } = carrier {
                            last_submitted[s][class] = Some(t);
                            if kinds[class].candidate_at(t) {
                                out.candidate_proofs += 1;
                            } else {
                                out.counted_proofs += 1;
                            }
                        }
                        pending[s] = Some(carrier);
                        inflight += 1;
                    }
                }
                last_lane[s] = slots.finish(inflight);
            }
        }
        out
    }

    /// One design in one network, as the network test prints it.
    #[derive(Debug)]
    struct Measured {
        ready: f64,
        paid: f64,
        jury: f64,
        counted_lapsed: u64,
        /// Post-admission steps with fewer than `seat_count` fresh rows, over eleven admissions.
        undrawable: u64,
        proofs_a_period: f64,
        court: u64,
    }

    fn measure(kinds: &[Kind], lane: Lane, shape: Shape, admissions: bool) -> Measured {
        const TICKS: u64 = 2_000;
        let n = network(8, kinds, TICKS, shape, lane);
        let (ready, paid, jury) = n.at_audits(100, 8);
        let mut undrawable = 0;
        if admissions {
            for at in (500..=1_500).step_by(PERIOD as usize) {
                let mut admitted = kinds.to_vec();
                let last = admitted.len() - 1;
                admitted[last] = Kind::Candidate { offset: 0, admitted: Some(at) };
                undrawable += network(8, &admitted, at + 41, shape, lane).admitted_undrawable;
            }
        }
        Measured {
            ready,
            paid,
            jury,
            counted_lapsed: n.counted_lapsed,
            undrawable,
            proofs_a_period: n.candidate_proofs as f64 * PERIOD as f64 / TICKS as f64 / 8.0,
            court: n.court,
        }
    }

    /// **Eight seats prove a Candidate through one pool** (the review's MEDIUM 2) — M1's network model
    /// with P3's clock, testnet-12's eight genesis seats all holding the class, `h` proofs a block
    /// (M1 measured two typical A16 proofs a block and no third), 2,000 DAA (twenty audits), and for
    /// the admission eleven runs each admitting the class at an audit and reading its rows for fifty
    /// DAA. Printed for the operator; the asserts pin what the choices rest on:
    ///
    /// * **Capacity**: the pool drains a tier first come, so what bounds the rows landed by an audit is
    ///   the first span any seat may send in. From `S − 6` (half the seats) the blocks `S − 5 … S − 2`
    ///   carry them: about `min(8, 4h)` rows fresh at the audit and `min(8, 3h)` paid — 8 and 6 at
    ///   two a block; the reviewed `S − 5` for every seat, `min(8, 3h)` and `min(8, 2h)` — 6 and 4.
    /// * **The admission**: a row from `S − 6` stands only until the hand-off carried by `S + 1` is
    ///   read, so every seat at `S − 6` (the fullest capacity) leaves `h` fresh rows at `S + 3` — a
    ///   class that went on to `Probation` is `Held` there at `h < seat_count`; half the seats at
    ///   `S − 5` keep theirs a span longer. Per-seat stagger inside the window adds no capacity — it is
    ///   what buys the admission its span.
    /// * **What P3 costs**: at two a block the aligned renewal after an admission is a burst today's
    ///   staggered cadence does not have (printed: undrawable steps); under a DA storm (one head a
    ///   block) a Candidate's rows reach the jury at the head's rate, about four in a window, where the
    ///   node without P3 — every row re-proved all period — reaches about as many at M1's shipped copy
    ///   guard and more with the guard one DAA longer (printed as "what if"); in exchange the seats'
    ///   court carriers stop waiting behind Candidate proofs (asserted: at least twice the court).
    /// * **Counted rows**: P3 never lapses more counted rows than the node without it.
    #[test]
    fn eight_seats_prove_a_candidate_through_one_pool() {
        let designs =
            [("without P3", TODAY), ("reviewed", REVIEWED), ("S − 5 for all", AT_5), ("S − 6 for all", AT_6), ("shipped", SHIPPED)];
        let cand = [Kind::Candidate { offset: 0, admitted: None }];
        let with_counted = [Kind::Counted, Kind::Candidate { offset: 0, admitted: None }];
        let print = |what: &str, name: &str, m: &Measured| {
            println!(
                "{what}, {name}: fresh {:.2} of 8 at the audit, paid {:.2}, P(jury seats) {:.2}; counted row-DAA lapsed {}; \
                 undrawable steps after 11 admissions {}; {:.1} Candidate proofs a period a seat; court carriers {}",
                m.ready, m.paid, m.jury, m.counted_lapsed, m.undrawable, m.proofs_a_period, m.court
            );
        };
        for (what, lane, h) in [("alone, 2 proofs a block", Lane::Open(2), 2.0f64), ("alone, 3 proofs a block", Lane::Open(3), 3.0f64)]
        {
            let m: Vec<Measured> = designs
                .iter()
                .map(|(name, shape)| {
                    let m = measure(&cand, lane, *shape, true);
                    print(what, name, &m);
                    m
                })
                .collect();
            let [today, _, at_5, at_6, shipped] = &m[..] else { unreachable!() };
            assert!(shipped.ready >= (4.0 * h).min(8.0) - 0.25 && shipped.paid >= (3.0 * h).min(8.0) - 0.25, "{what}: {shipped:?}");
            assert!(at_5.ready <= (3.0 * h).min(8.0) && at_5.paid <= (2.0 * h).min(8.0), "{what}: {at_5:?}");
            assert!(shipped.ready >= today.ready && shipped.paid > today.paid, "{what}: {shipped:?} vs {today:?}");
            assert!(shipped.undrawable < at_6.undrawable, "{what}: the stagger buys the admission a span: {shipped:?} vs {at_6:?}");
            assert!(shipped.proofs_a_period <= 2.0 && today.proofs_a_period >= 20.0, "{what}: two proofs a period, not twenty");
            if h >= 3.0 {
                assert_eq!(shipped.undrawable, 0, "{what}: no undrawable step after an admission");
            }
        }
        let storm = Lane::Storm { beside: 1 };
        let today = measure(&cand, storm, TODAY, false);
        let shipped = measure(&cand, storm, SHIPPED, false);
        print("alone, DA storm (one head a block)", "without P3", &today);
        print("alone, DA storm (one head a block)", "shipped", &shipped);
        assert!(shipped.court >= 2 * today.court, "the court stops waiting behind Candidate proofs: {shipped:?} vs {today:?}");
        for (name, shape) in [("without P3", TODAY), ("shipped", SHIPPED)] {
            let m = measure(&cand, storm, Shape { m1_guard_plus: true, ..shape }, false);
            print("what if M1's copy guard held one DAA longer — alone, DA storm", name, &m);
        }
        for (what, lane) in
            [("beside a counted class, 3 proofs a block", Lane::Open(3)), ("beside a counted class, 4 proofs a block", Lane::Open(4))]
        {
            let m: Vec<Measured> = designs
                .iter()
                .map(|(name, shape)| {
                    let m = measure(&with_counted, lane, *shape, false);
                    print(what, name, &m);
                    m
                })
                .collect();
            assert!(m[4].counted_lapsed <= m[0].counted_lapsed, "{what}: {:?} vs {:?}", m[4], m[0]);
        }
    }

    /// **The panel reads the plan, not a copy of it**: `readiness_duties` keeps today's duty and M1's
    /// copy guard as one condition (`today_holds`, the text M1's own pin reads) and M1's urgency as it
    /// was, then asks `palw_candidate_proof_plan_v1` for every class — armed by this module's R-core+
    /// reader at the tick's DAA, the class's lifecycle `Candidate` at the tip, the registry's own row
    /// age, the fold's period and audit predicate, this bond's stagger — before it builds anything:
    /// `Hold` notes the class and builds no proof, `Send` builds one for the Own site (no urgency) and
    /// keeps its rank, `Today` is today's duty with M1's urgency. The duties go to the tick in
    /// [`palw_candidate_own_order_v1`]'s order. Every proof it builds after that is the one it always
    /// built.
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
            "|audit_span| crate::palw_candidate_proof_timing::palw_candidate_stagger_v1(&bond_bytes, &class.class_id, audit_span),",
            "palw_candidate_sharing_v1(&class.class_id, &candidates_held, audit_span, |id, span| { \
             crate::palw_candidate_proof_timing::palw_candidate_audit_due_v1(&self.consensus_config.params, id, span, audit_period,) },)",
            "let urgency = match candidate {",
            "PalwCandidateProofPlanV1::Hold { .. } => { self.readiness_note(class.class_id, candidate.note()); continue; }",
            "PalwCandidateProofPlanV1::Send { rank, .. } => { candidate_sends.push((class.class_id, rank)); None }",
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
                "crate::palw_candidate_proof_timing::palw_candidate_own_order_v1(out, |duty| duty.class_id().and_then(\
                 |class_id| candidate_sends.iter().find(|(id, _)| *id == class_id).map(|(_, rank)| *rank)), |duty| duty.escalates(),)"
            )),
            "the tick gets the duties in the Own order"
        );
        assert!(duties.find("let bond_bytes = borsh::to_vec(&bond_key)").is_some_and(|at| at < plan), "the stagger's bond");
        let held = duties.find("let candidates_held: Vec<Hash64> = read").expect("the seat's Candidates");
        assert!(
            squeeze(&duties[held..plan])
                .contains(&squeeze("&& read.readiness.iter().any(|r| r.bond == bond_key && r.class_id == c.class_id)")),
            "a Candidate this bond has a row for"
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
