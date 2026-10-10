//! **Lane LG14-A (RFC-0014 §6): the common fraud filer, the node's half** (node policy; a child module of `palw_panel`, whose loop
//! holds its one book and calls [`PalwPanelService::fraud_filer_tick_v1`] once a tick).
//!
//! **What it closes** (GAP-80, G14 on the legacy V2 Panel route). A claim every seat of its panel signed `Valid` reaches `Final` unless
//! someone OUTSIDE the panel can take it to an objective outcome. Past `palw_legacy_public_filer_v1` any bond can: this node replays
//! the claim's job on its own model (the attempt's job is derived from the claim's block, never served), and when its execution root
//! differs it runs the ONE engine every role runs (`kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1`, also
//! what the real-node suite `lg14a_legacy_filer_e2e` drives):
//!
//! 1. **reserve** — tag 154, asked of the fold first (`palw_legacy_dispute_reservation_check_v1`): the claim's ends are held until its
//!    own hard deadline, so no timeout outruns the pursuit;
//! 2. **read the binding** — a reserved `DefaultAccused` of event `(0, 0)`, whose answer carries the claim's step binding (the fold
//!    authenticates it against the claim's `execution_root`);
//! 3. **localize** — reserved `StepRange` demands compare consecutive ranges of ≤ 1,024 leaves with this node's own run until the
//!    first difference is located. Ladders exceeding the reservation's session cap are Unjudged by this fallback;
//! 4. **terminal** — a reserved `StepLeaf` demand of that leaf: the producer's evidence is adjudicated (guilty: `CourtFraud`), its
//!    silence defaults (DA-7). A fused leaf is the held dissection's (lane LG14-B), said and left.
//!
//! **Shared progress** (§7.4): a unit the chain already answered — this node's before a restart, or anybody's — is read off the
//! accepted blocks, never demanded again; the unit must be marked answered and the consumed bytes must authenticate to the claim.
//!
//! **Restart** (§6.3): the book is a cache, nothing is persisted. The candidates and every case's chain facts come from the tip, the
//! answers from the accepted blocks (walked back to the oldest pursued claim's acceptance after a start), and the honest run from a
//! fresh replay. A restart costs one replay a case and never a wrong filing.
//!
//! **Who** — identity decides, nothing opts in: a node that carries (`--palw-fee-outpoint`) past the fence. A claim on whose current
//! panel this bond sits is the seat's duties' (P2-6, P2-8b/8d), which this filer leaves alone so one bond never files a claim twice;
//! its own claims are never pursued.
//!
//! **Load.** One replay in flight, started only while the seat's own replay slots have room for a light replay; a mismatch's capture is
//! kept for its localization, at most [`PALW_FRAUD_FILER_HELD_RUNS_V1`] at once (the rest wait their turn); one item on the court queue a
//! case, each sent at most [`PALW_FRAUD_FILER_SENDS_V1`] times; the chain read once a DAA. Every item rides the priority lane dated
//! now: a reservation and a demand are what keep the claim's ends held and the producer's answer window running.
//!
//! **Consensus-inert, and dormant below the fence** — a clean no-op there: the book is emptied and nothing is queued, so a node below
//! `palw_legacy_public_filer_v1` (every shipped preset) behaves byte for byte as before.

use super::*;
use std::collections::BTreeSet;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_step_leaf_is_fused_v1};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, PalwFilerActionV1, PalwFilerPhaseV1, PalwFilerRoleV1, PalwLegacyBisectV1,
    PalwLegacyProbeV1, palw_dispute_reserved_object_v1, palw_fraud_filer_demand_object_v1, palw_fraud_filer_facts_v1,
    palw_fraud_filer_learn_v1, palw_fraud_filer_next_v1, palw_fraud_filer_reservation_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwFraudFilerCandidateV1, PalwLegacyDisputeViewV1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_core::{debug, error};

/// The court queue's round of this filer's reservation of a claim — a round no other lane keys by (P2-6 `u32::MAX`, the reporter
/// filer `u32::MAX − 3 ..= u32::MAX − 1`, P2-8d `− 6`, lane B `− 7`, the audit duty `− 8`, the rider outbox `− 9`).
pub(super) const PALW_FRAUD_FILER_RESERVE_ROUND_V1: u32 = u32::MAX - 10;
/// The court queue's round of this filer's demand on a claim (one open at a time: DA-1's one session per accuser).
pub(super) const PALW_FRAUD_FILER_DEMAND_ROUND_V1: u32 = u32::MAX - 11;
/// The host ledger's role for this filer's replays.
pub(super) const PALW_FRAUD_FILER_REPLAY_ROLE_V1: &str = "fraud-filer";
/// Mismatched runs held in memory at once (each keeps its capture for the localizer's own ranges).
pub(super) const PALW_FRAUD_FILER_HELD_RUNS_V1: usize = 2;
/// Replays one claim gets: the run, and one more after a failure that was this host's.
pub(super) const PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1: u8 = 2;
/// Sends of one item (a reservation, one probe's demand): the first, and one more if the first was lost.
pub(super) const PALW_FRAUD_FILER_SENDS_V1: u8 = 2;
/// A queued item not on chain this long after it was sent is taken as lost.
pub(super) const PALW_FRAUD_FILER_RESEND_DAA_V1: u64 = 30;
/// Chain blocks one walk of the accepted objects reads at most (the first walk after a start reaches back to the oldest pursued
/// claim's acceptance; later walks read what the tip added, plus a reorg margin).
pub(super) const PALW_FRAUD_FILER_WALK_BLOCKS_V1: usize = 40_000;
/// DAA a later walk re-reads below the last one's tip, for a reorg.
pub(super) const PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1: u64 = 64;
/// Cases the book keeps at most (settled ones are dropped first once their claim leaves the candidates).
pub(super) const PALW_FRAUD_FILER_MAX_CASES_V1: usize = 4_096;

/// Whether a court-queue entry is this filer's.
pub(super) fn palw_fraud_filer_queued_v1(round: u32, responder: bool) -> bool {
    !responder && (round == PALW_FRAUD_FILER_RESERVE_ROUND_V1 || round == PALW_FRAUD_FILER_DEMAND_ROUND_V1)
}

/// **One honest run of a claim's job on this host** — the filer's own material, never the producer's: the roots its replay
/// reproduced, and its own leaf hashes of any range (opened from its capture by the backend that ran it, which the closure holds with
/// the capture and the prompt). Holds the ledger's reservation for as long as the capture lives.
pub(super) struct PalwFraudFilerRunV1 {
    pub(super) execution_root: Hash64,
    pub(super) trace_root: Hash64,
    /// This node's own leaf hashes of `[first, first + count)`.
    pub(super) own_range: Box<dyn Fn(u64, u32) -> Result<Vec<Hash64>, String> + Send + Sync>,
    pub(super) _reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// Where this node's judgement of one claim stands.
#[derive(Clone)]
pub(super) enum PalwFraudFilerVerdictV1 {
    /// Not yet replayed (or a replay that failed on this host, to be run once more).
    Pending,
    /// The replay reproduced the claim's execution root: nothing to file.
    Honest,
    /// The replay did not: the engine pursues it.
    Mismatch(Arc<PalwFraudFilerRunV1>),
    /// This host cannot judge it (the class does not resolve, the block is not held, the replay failed twice, a fused leaf).
    Unjudged(String),
    /// The pursuit is over.
    Settled(PalwFilerPhaseV1),
}

/// What this node sent last on a case, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerSentV1 {
    Reserve,
    Demand(PalwLegacyProbeV1),
}

/// **One case** — a candidate claim and this node's pursuit of it.
pub(super) struct PalwFraudFilerCaseV1 {
    pub(super) candidate: PalwFraudFilerCandidateV1,
    pub(super) verdict: PalwFraudFilerVerdictV1,
    pub(super) binding: Option<PalwStepBindingV2>,
    pub(super) bisect: PalwLegacyBisectV1,
    pub(super) runs: u8,
    /// The last item sent, when, and how many times.
    pub(super) sent: Option<(PalwFraudFilerSentV1, u64, u8)>,
}

impl PalwFraudFilerCaseV1 {
    fn new(candidate: PalwFraudFilerCandidateV1) -> Self {
        Self {
            candidate,
            verdict: PalwFraudFilerVerdictV1::Pending,
            binding: None,
            bisect: PalwLegacyBisectV1::new(0),
            runs: 0,
            sent: None,
        }
    }

    fn pursued(&self) -> bool {
        matches!(self.verdict, PalwFraudFilerVerdictV1::Mismatch(_))
    }

    /// May `item` be sent now? A different item always; the same one only once [`PALW_FRAUD_FILER_RESEND_DAA_V1`] passed since it was
    /// sent and fewer than [`PALW_FRAUD_FILER_SENDS_V1`] sends were made. `Err` once the sends are spent.
    fn may_send(&self, item: PalwFraudFilerSentV1, now_daa: u64) -> Result<bool, ()> {
        match self.sent {
            Some((last, at, sends)) if last == item => {
                if now_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
                    Ok(false)
                } else if sends >= PALW_FRAUD_FILER_SENDS_V1 {
                    Err(())
                } else {
                    Ok(true)
                }
            }
            _ => Ok(true),
        }
    }

    fn note_sent(&mut self, item: PalwFraudFilerSentV1, now_daa: u64) {
        let sends = match self.sent {
            Some((last, _, sends)) if last == item => sends.saturating_add(1),
            _ => 1,
        };
        self.sent = Some((item, now_daa, sends));
    }
}

/// **The filer's book** (the module's header): the cases by claim, the replay in flight, and the answers read off the accepted blocks.
#[derive(Default)]
pub(super) struct PalwFraudFilerBookV1 {
    pub(super) cases: BTreeMap<Hash64, PalwFraudFilerCaseV1>,
    read_at: Option<u64>,
    running: Option<(Hash64, tokio::task::JoinHandle<Result<PalwFraudFilerRunV1, String>>)>,
    /// `(claim, unit) → answer`, every `MaterialDisclosedV2` the accepted blocks carried for a pursued claim.
    pub(super) answers: BTreeMap<(Hash64, PalwDaUnitV1), PalwDaAnswerV1>,
    /// The DAA the walks have read down to (`None`: never walked since the start).
    walked_from: Option<u64>,
    /// The tip's DAA at the last walk.
    walked_to: Option<u64>,
    /// This bond's role (an operator's genesis bond, or any other) — read once per refresh.
    role: Option<PalwFilerRoleV1>,
}

impl PalwFraudFilerBookV1 {
    /// Forget everything (below the fence, or a node that does not carry).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// **Take a fresh candidate read**: a new claim opens a case; a case whose claim left the candidates is dropped unless it is
    /// pursued (its outcome is read off the tip); a case's job facts follow the tip. Bounded by [`PALW_FRAUD_FILER_MAX_CASES_V1`].
    pub(super) fn refresh(&mut self, candidates: Vec<PalwFraudFilerCandidateV1>, now_daa: u64) {
        let fresh: BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        self.cases.retain(|claim, case| fresh.contains(claim) || case.pursued());
        for candidate in candidates {
            let room = self.cases.len() < PALW_FRAUD_FILER_MAX_CASES_V1;
            match self.cases.get_mut(&candidate.claim_id) {
                Some(case) => case.candidate = candidate,
                None if room => {
                    self.cases.insert(candidate.claim_id, PalwFraudFilerCaseV1::new(candidate));
                }
                None => {}
            }
        }
        self.read_at = Some(now_daa);
    }

    /// Whether the chain should be read again at `now_daa`.
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// Mismatched runs held now.
    pub(super) fn held_runs(&self) -> usize {
        self.cases.values().filter(|case| case.pursued()).count()
    }

    /// **The next claim to replay**: none while one runs or the held runs are at their cap; else the oldest pending case this bond
    /// files as a non-seat (not a seat of its panel), oldest acceptance first.
    pub(super) fn next_replay(&self) -> Option<Hash64> {
        if self.running.is_some() || self.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1 {
            return None;
        }
        self.cases
            .values()
            .filter(|case| matches!(case.verdict, PalwFraudFilerVerdictV1::Pending) && !case.candidate.seat)
            .min_by_key(|case| (case.candidate.accepted_daa, case.candidate.claim_id))
            .map(|case| case.candidate.claim_id)
    }

    /// The claims the engine steps this tick, oldest acceptance first.
    pub(super) fn pursued(&self) -> Vec<Hash64> {
        let mut out: Vec<(u64, Hash64)> = self
            .cases
            .values()
            .filter(|case| case.pursued())
            .map(|case| (case.candidate.accepted_daa, case.candidate.claim_id))
            .collect();
        out.sort();
        out.into_iter().map(|(_, claim)| claim).collect()
    }

    /// **Judge a returned replay** against the claim's committed roots: the execution root reproduces — honest; it does not — the
    /// case is pursued; the execution reproduces and the trace does not — not this filer's (P2-6's event demands); a failure on this
    /// host — run once more, then unjudged.
    pub(super) fn judge(&mut self, claim: Hash64, result: Result<PalwFraudFilerRunV1, String>) -> Option<&PalwFraudFilerVerdictV1> {
        let case = self.cases.get_mut(&claim)?;
        case.runs = case.runs.saturating_add(1);
        case.verdict = match result {
            Ok(run) if run.execution_root != case.candidate.job.execution_root => PalwFraudFilerVerdictV1::Mismatch(Arc::new(run)),
            Ok(run) if run.trace_root != case.candidate.job.trace_root => {
                PalwFraudFilerVerdictV1::Unjudged("the execution reproduces and the trace does not (P2-6's event demands)".into())
            }
            Ok(_) => PalwFraudFilerVerdictV1::Honest,
            Err(_) if case.runs < PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1 => PalwFraudFilerVerdictV1::Pending,
            Err(why) => PalwFraudFilerVerdictV1::Unjudged(format!("the replay failed twice: {why}")),
        };
        Some(&case.verdict)
    }

    /// **Where the next walk of the accepted objects starts**, or `None` when nothing is pursued: the oldest pursued claim's
    /// acceptance on the first walk (or once a newly pursued claim is older than what was walked), else the last walk's tip less a
    /// reorg margin.
    pub(super) fn walk_floor(&self) -> Option<u64> {
        let oldest = self.cases.values().filter(|case| case.pursued()).map(|case| case.candidate.accepted_daa).min()?;
        Some(match (self.walked_from, self.walked_to) {
            (Some(from), Some(to)) if from <= oldest => to.saturating_sub(PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1),
            _ => oldest,
        })
    }

    /// Record a walk from `floor` to the tip at `tip_daa` and the answers it read for pursued claims.
    pub(super) fn walked(&mut self, floor: u64, tip_daa: u64, answers: Vec<((Hash64, PalwDaUnitV1), PalwDaAnswerV1)>) {
        self.walked_from = Some(self.walked_from.map_or(floor, |from| from.min(floor)));
        self.walked_to = Some(tip_daa);
        for (key, answer) in answers {
            if self.cases.get(&key.0).is_some_and(|case| {
                case.pursued()
                    && kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                        &key.1,
                        &answer,
                        &case.candidate.job.execution_root,
                    )
            }) {
                self.answers.entry(key).or_insert(answer);
            }
        }
        // An answer of a claim no longer pursued leaves with its case.
        let pursued: BTreeSet<Hash64> = self.pursued().into_iter().collect();
        self.answers.retain(|(claim, _), _| pursued.contains(claim));
    }

    /// Settle a case: its run (and the capture's reservation) is let go.
    pub(super) fn settle(&mut self, claim: &Hash64, verdict: PalwFraudFilerVerdictV1) {
        if let Some(case) = self.cases.get_mut(claim) {
            case.verdict = verdict;
        }
    }
}

/// **The engine's step for one case**, decided off what the chain says and what this node already read (pure, so the tests drive it):
/// the engine's own action, except that a demand the chain already answered is READ (shared progress, §7.4) — `Learn` — and one it
/// has not answered while this node holds no answer read for it yet is demanded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerStepV1 {
    Engine(PalwFilerActionV1),
    Learn(PalwLegacyProbeV1),
    /// The chain marks the unit answered but the walk has not read the answer yet.
    AwaitAnswer(PalwLegacyProbeV1),
}

pub(super) fn palw_fraud_filer_step_v1(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let binding = case.binding.as_ref();
    let action = palw_fraud_filer_next_v1(role, &facts, case.pursued(), &case.bisect, |leaf| {
        binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf))
    });
    match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.answered.contains(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                (false, _) => PalwFraudFilerStepV1::Engine(action),
            }
        }
        action => PalwFraudFilerStepV1::Engine(action),
    }
}

impl PalwPanelService {
    /// **The filer's half of the tick** (the module's header): read the candidates and the accepted answers once a DAA, poll and judge
    /// the replay in flight, start the next one where the seat's slots have room, and step every pursued case through the engine —
    /// each item asked of the fold or built by the one builder, queued on the court lane dated now.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fraud_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        // What the priority lane carried of this filer's items since the last tick, said and forgotten here: the filer keeps its own
        // spacing (`sent`), never `court_moved`'s debounce.
        court_moved.retain(|(claim, round, responder), at| {
            if !palw_fraud_filer_queued_v1(*round, *responder) {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: the fraud filer's item (round {round}) was carried at DAA {at} (LG14-A)");
            false
        });
        let params = &self.consensus_config.params;
        if !(params.palw_legacy_public_filer_active_at(current_daa) && self.config.fee_outpoint.is_some()) {
            if !book.cases.is_empty() || book.running.is_some() {
                book.clear();
            }
            court_pending.retain(|(_, round, responder, _)| !palw_fraud_filer_queued_v1(*round, *responder));
            return;
        }
        if book.stale(current_daa) {
            let candidates = session.clone().spawn_blocking(move |c| c.palw_fraud_filer_candidates_v1(bond_key)).await;
            book.refresh(candidates, current_daa);
            if book.role.is_none() {
                let operator = palw_operator_da::palw_operator_registrations_v1(params).iter().any(|(bond, _)| *bond == bond_key);
                book.role = Some(if operator { PalwFilerRoleV1::Operator } else { PalwFilerRoleV1::PublicBond });
            }
            if let Some(floor) = book.walk_floor() {
                let wanted: BTreeSet<Hash64> = book.pursued().into_iter().collect();
                let span = current_daa.saturating_sub(floor).saturating_add(PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1);
                let span = usize::try_from(span).unwrap_or(usize::MAX).min(PALW_FRAUD_FILER_WALK_BLOCKS_V1);
                let answers = session
                    .clone()
                    .spawn_blocking(move |c| {
                        let mut answers = Vec::new();
                        crate::palw_panel::walk_accepted_lifecycle_objects_v1(c, floor, span, &mut |object| {
                            if let PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, .. } = object
                                && wanted.contains(&claim)
                            {
                                answers.push(((claim, unit), answer));
                            }
                        });
                        answers
                    })
                    .await;
                book.walked(floor, current_daa, answers);
            }
        }
        // The replay in flight, polled every tick until it returns.
        if book.running.as_ref().is_some_and(|(_, handle)| handle.is_finished()) {
            let (claim, handle) = book.running.take().expect("held above");
            let result = handle.await.unwrap_or_else(|e| Err(format!("the replay task did not finish: {e}")));
            match book.judge(claim, result) {
                Some(PalwFraudFilerVerdictV1::Mismatch(run)) => error!(
                    "[{PALW_PANEL}] claim {claim}: this node's replay of the claim's job does NOT reproduce its execution root (replayed \
                     {}) — every seat of its panel let it pass or none looked; pursuing it as a public bond (LG14-A, RFC-0014 §6)",
                    run.execution_root
                ),
                Some(PalwFraudFilerVerdictV1::Honest) => {
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay reproduces the claim — honest, nothing filed")
                }
                Some(PalwFraudFilerVerdictV1::Unjudged(why)) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot judge the claim — {why} (LG14-A)")
                }
                _ => {}
            }
        }
        if seat_replay_room && let Some(claim) = book.next_replay() {
            self.fraud_filer_start_replay_v1(session, book, claim, network_domain);
        }
        // Every pursued case, one engine step each (a step that reads an answer runs on to the next).
        let role = book.role.unwrap_or(PalwFilerRoleV1::PublicBond);
        let form = params.palw_prompt_ids_form_at(current_daa);
        for claim in book.pursued() {
            if court_pending
                .iter()
                .any(|(queued, round, responder, _)| *queued == claim && palw_fraud_filer_queued_v1(*round, *responder))
            {
                continue;
            }
            let (view, reservable) = session
                .clone()
                .spawn_blocking(move |c| {
                    let view = c.palw_legacy_dispute_v1(claim);
                    let reservable = view
                        .as_ref()
                        .and_then(|view| c.palw_legacy_dispute_reservation_check_v1(palw_fraud_filer_reservation_v1(view, bond_key)))
                        .is_some_and(|check: Result<u128, String>| check.is_ok());
                    (view, reservable)
                })
                .await;
            for _ in 0..4 {
                let Some(case) = book.cases.get(&claim) else { break };
                let step = palw_fraud_filer_step_v1(role, view.as_ref(), &bond_key, reservable, case, |unit| {
                    book.answers.contains_key(&(claim, *unit))
                });
                match step {
                    PalwFraudFilerStepV1::Learn(probe) => {
                        let answer = book.answers.get(&(claim, probe.unit())).cloned().expect("read above");
                        let Some(view) = view.as_ref() else { break };
                        let PalwFraudFilerVerdictV1::Mismatch(run) = case.verdict.clone() else { break };
                        let (mut binding, mut bisect) = (case.binding.clone(), case.bisect);
                        let execution_root = view.execution_root;
                        let learned = tokio::task::spawn_blocking(move || {
                            palw_fraud_filer_learn_v1(probe, &answer, &execution_root, &mut binding, &mut bisect, |first, count| {
                                (run.own_range)(first, count)
                            })
                            .map(|()| (binding, bisect))
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("the read did not finish: {e}")));
                        match learned {
                            Ok((binding, bisect)) => {
                                let case = book.cases.get_mut(&claim).expect("held above");
                                case.binding = binding;
                                case.bisect = bisect;
                                debug!(
                                    "[{PALW_PANEL}] claim {claim}: read the chain's answer to {probe:?} — the interval is {bisect:?}"
                                );
                            }
                            Err(why) => {
                                warn!("[{PALW_PANEL}] claim {claim}: the chain's answer to {probe:?} does not read: {why} (LG14-A)");
                                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
                                break;
                            }
                        }
                    }
                    PalwFraudFilerStepV1::AwaitAnswer(_) | PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait) => break,
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(phase)) => {
                        match phase {
                            PalwFilerPhaseV1::Convicted | PalwFilerPhaseV1::DaDefault => {
                                info!("[{PALW_PANEL}] claim {claim}: the pursuit ended — {phase:?} (LG14-A)")
                            }
                            _ => warn!(
                                "[{PALW_PANEL}] claim {claim}: the pursuit ended without an objective outcome — {phase:?} (LG14-A)"
                            ),
                        }
                        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(phase));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::HeldRoute { leaf }) => {
                        warn!(
                            "[{PALW_PANEL}] claim {claim}: the first divergent leaf {leaf} is a fused-attention site — its terminal is the \
                             held dissection's (LG14-B), not a unit; the reservation keeps the claim held until its hard deadline"
                        );
                        book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("fused leaf {leaf}: the held dissection's")));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve) => {
                        let Some(view) = view.as_ref() else { break };
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Reserve,
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                let mut signed = true;
                                let object = palw_dispute_reserved_object_v1(
                                    network_domain,
                                    palw_fraud_filer_reservation_v1(view, bond_key),
                                    |m| {
                                        self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                            signed = false;
                                            Vec::new()
                                        })
                                    },
                                );
                                if signed { Ok(object) } else { Err("no signing key for this bond".to_string()) }
                            },
                        );
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)) => {
                        let Some(view) = view.as_ref() else { break };
                        let binding = case.binding.clone();
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Demand(probe),
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                palw_fraud_filer_demand_object_v1(
                                    &network_domain,
                                    claim,
                                    &view.execution_root,
                                    probe,
                                    binding.as_ref(),
                                    bond_key,
                                    form,
                                    |message, context| self.sign(message, context),
                                )
                            },
                        );
                        break;
                    }
                }
            }
        }
    }

    /// **Queue one item of a case** under the filer's own key, dated now — once per [`PALW_FRAUD_FILER_RESEND_DAA_V1`] and at most
    /// [`PALW_FRAUD_FILER_SENDS_V1`] times; an item whose sends are spent, or that cannot be built, ends this node's pursuit loudly.
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_queue_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        item: PalwFraudFilerSentV1,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        build: impl FnOnce() -> Result<PalwConsensusObjectV2, String>,
    ) {
        let Some(case) = book.cases.get_mut(&claim) else { return };
        match case.may_send(item, current_daa) {
            Ok(false) => return,
            Ok(true) => {}
            Err(()) => {
                warn!(
                    "[{PALW_PANEL}] claim {claim}: the fraud filer's {item:?} was sent {PALW_FRAUD_FILER_SENDS_V1} times and never landed (LG14-A)"
                );
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("{item:?} never landed")));
                return;
            }
        }
        match build() {
            Ok(object) => {
                let round = match item {
                    PalwFraudFilerSentV1::Reserve => PALW_FRAUD_FILER_RESERVE_ROUND_V1,
                    PalwFraudFilerSentV1::Demand(_) => PALW_FRAUD_FILER_DEMAND_ROUND_V1,
                };
                info!("[{PALW_PANEL}] claim {claim}: the fraud filer queues {item:?} (LG14-A, RFC-0014 §6–§7)");
                court_due.insert((claim, round, false), current_daa);
                court_pending.push((claim, round, false, object));
                case.note_sent(item, current_daa);
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot build {item:?}: {why} (LG14-A)");
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            }
        }
    }

    /// **Start this node's replay of `claim`'s job** off the loop — the anchor's job derived from the claim's block
    /// (`attempt_job_for_claim`), on the class's own backend, under the host ledger's reservation at the full seat's need (lane B's
    /// replay, kept whole: the capture is what this node's own ranges are opened from). A claim this host cannot replay is unjudged.
    fn fraud_filer_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        network_domain: Hash64,
    ) {
        let Some(case) = book.cases.get(&claim) else { return };
        let job = case.candidate.job.clone();
        let producer = case.candidate.producer;
        let unjudged = |book: &mut PalwFraudFilerBookV1, why: String| {
            debug!("[{PALW_PANEL}] claim {claim}: the fraud filer does not judge it — {why}");
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
        };
        if job.free_prompt {
            return unjudged(book, "a free prompt's job is its caller's, served, never derived".into());
        }
        let Some(artifact_root) = job.artifact_root else { return unjudged(book, "its class is gone from the registry".into()) };
        let mut backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => return unjudged(book, format!("its class does not resolve on this host ({why})")),
        };
        let Some((ctx, prompt)) =
            self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &producer)
        else {
            return unjudged(book, "its block's header is not held here".into());
        };
        let backends = self.backends();
        let reserved =
            match self.reserve_replay_at_widest_run_v1(PALW_FRAUD_FILER_REPLAY_ROLE_V1, backend.as_mut(), job.class_id, claim, |b| {
                backends.role_memory_need_for_backend_or_chain_v1(
                    b,
                    job.class_id,
                    artifact_root,
                    Some(&ctx),
                    super::FULL_SEAT_V1,
                    |id| self.chain_carriage_v1(session, id),
                )
            }) {
                Ok((reserved, _)) => reserved,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("panel-fraud-filer-ledger", || {
                        format!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay waits: {why} (LG14-A)")
                    });
                    return;
                }
            };
        let Ok(prompt32) = prompt.iter().map(|id| u32::try_from(*id)).collect::<Result<Vec<u32>, _>>() else {
            return unjudged(book, "a prompt id past u32".into());
        };
        info!("[{PALW_PANEL}] claim {claim}: the fraud filer replays the claim's job off the loop (LG14-A, RFC-0014 §6.1)");
        let handle = tokio::task::spawn_blocking(move || {
            let outcome = backend.execute(&ctx, &prompt)?;
            let backend: Arc<dyn PalwExecutionBackendV1> = Arc::from(backend);
            let material = outcome.material;
            Ok(PalwFraudFilerRunV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                own_range: Box::new(move |first, count| {
                    backend.held_step_range_answer_v1(&material, &prompt32, first, count).map(|opening| opening.leaf_hashes)
                }),
                _reservation: Some(reserved),
            })
        });
        book.running = Some((claim, handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
    use kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1;
    use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xF1), n as u32))
    }

    fn candidate(claim: u64, accepted_daa: u64, seat: bool) -> PalwFraudFilerCandidateV1 {
        PalwFraudFilerCandidateV1 {
            claim_id: Hash64::from_u64_word(claim),
            producer: bond(99),
            accepted_daa,
            seat,
            job: PalwOperatorDaJobV1 {
                accepted_block: Hash64::from_u64_word(claim + 1_000),
                class_id: Hash64::from_u64_word(7),
                artifact_root: Some(Hash64::from_u64_word(8)),
                execution_root: Hash64::from_u64_word(0xE0 + claim),
                trace_root: Hash64::from_u64_word(0x70 + claim),
                output_root: Hash64::default(),
                work_leaves: 0,
                free_prompt: false,
                held_to_final: false,
            },
        }
    }

    fn view(claim: u64, phase: PalwClaimPhaseV2) -> PalwLegacyDisputeViewV1 {
        PalwLegacyDisputeViewV1 {
            claim_id: Hash64::from_u64_word(claim),
            class_id: Hash64::from_u64_word(7),
            producer: bond(99),
            phase,
            execution_root: Hash64::from_u64_word(0xE0 + claim),
            trace_root: Hash64::from_u64_word(0x70 + claim),
            accepted_daa: 100,
            trace_retention_daa: 5_000,
            job_identity: Hash64::default(),
            work_leaves: 0,
            deadline_daa: None,
            hard_deadline_daa: 3_800,
            record: None,
            sessions: Vec::new(),
            answered: Vec::new(),
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    /// A run whose roots are `execution_root` / `trace_root`, opening no range (the tests never read one).
    fn run(execution_root: Hash64, trace_root: Hash64) -> PalwFraudFilerRunV1 {
        PalwFraudFilerRunV1 { execution_root, trace_root, own_range: Box::new(|_, _| Err("no range".into())), _reservation: None }
    }

    /// **The book replays oldest first, one at a time, never a seat's claim, and holds at most two mismatches.**
    #[test]
    fn the_book_replays_oldest_first_and_bounds_what_it_holds() {
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false), candidate(1, 100, true), candidate(2, 200, false), candidate(4, 400, false)], 10);
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(2)), "the seat's claim is the seat's duties'");
        let honest = candidate(2, 200, false).job;
        book.judge(Hash64::from_u64_word(2), Ok(run(honest.execution_root, honest.trace_root)));
        assert!(matches!(book.cases[&Hash64::from_u64_word(2)].verdict, PalwFraudFilerVerdictV1::Honest));
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(3)));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        assert_eq!(book.held_runs(), 2);
        book.refresh(vec![candidate(5, 50, false)], 11);
        assert_eq!(book.next_replay(), None, "two mismatches held: the next waits");
        assert_eq!(book.pursued(), vec![Hash64::from_u64_word(3), Hash64::from_u64_word(4)], "pursued cases survive the candidates");
        assert!(!book.cases.contains_key(&Hash64::from_u64_word(2)), "a settled case leaves with its candidate");
        // A failure on this host is run once more, then unjudged.
        book.settle(&Hash64::from_u64_word(3), PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        let five = Hash64::from_u64_word(5);
        assert_eq!(book.next_replay(), Some(five));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Pending));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
    }

    /// **Shared progress**: a probe the chain already answered is read (once the walk has the answer), never demanded; a reservation
    /// comes first; the engine's terminal is demanded even when answered (the fold adjudicates it).
    #[test]
    fn a_unit_the_chain_answered_is_read_never_demanded() {
        let me = bond(1);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false)], 10);
        let claim = Hash64::from_u64_word(3);
        book.judge(claim, Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let case = &book.cases[&claim];
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 150 });
        let step = |v: &PalwLegacyDisputeViewV1, reservable, read: bool| {
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, Some(v), &me, reservable, case, |_| read)
        };
        assert_eq!(step(&v, true, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve));
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 151, deposit: 1, sessions_opened: 0 });
        v.record = Some(record);
        let binding = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(binding)));
        v.answered.push(binding.unit());
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::AwaitAnswer(binding));
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Learn(binding));
        // The outcome ends it whatever else stands.
        v.phase =
            PalwClaimPhaseV2::Voided { voided_daa: 200, reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud };
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Convicted)));
        assert_eq!(
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, None, &me, false, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired)),
            "a retired claim's pursuit expires"
        );
    }

    /// **An item is sent once, re-sent once after it was lost, and never a third time**; a different item is never held back.
    #[test]
    fn an_item_is_sent_at_most_twice() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        let reserve = PalwFraudFilerSentV1::Reserve;
        assert_eq!(case.may_send(reserve, 10), Ok(true));
        case.note_sent(reserve, 10);
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1 - 1), Ok(false));
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1), Ok(true));
        case.note_sent(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1);
        assert_eq!(case.may_send(reserve, 10 + 3 * PALW_FRAUD_FILER_RESEND_DAA_V1), Err(()));
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 }), 11), Ok(true));
    }

    /// **The walk reads from the oldest pursued claim after a start, then only what the tip added (less a reorg margin).**
    #[test]
    fn the_walk_starts_at_the_oldest_pursuit_then_follows_the_tip() {
        let mut book = PalwFraudFilerBookV1::default();
        assert_eq!(book.walk_floor(), None, "nothing pursued: no walk");
        book.refresh(vec![candidate(3, 300, false), candidate(4, 400, false)], 10);
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(400));
        book.walked(400, 1_000, Vec::new());
        assert_eq!(book.walk_floor(), Some(1_000 - PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(300), "an older pursuit walks back to its acceptance");
    }

    /// **The tick wires the filer behind no flag, after lane B, with the seat's replay room** (a source pin).
    #[test]
    fn the_tick_wires_the_filer_behind_no_flag() {
        let panel = include_str!("palw_panel.rs");
        let tick = panel.find("self.fraud_filer_tick_v1(").expect("the tick calls the filer");
        let operator = panel.find("self.operator_da_tick_v1(").expect("lane B");
        assert!(tick > operator, "after lane B");
        assert!(panel[tick..tick + 400].contains("seat_replays.has_room(false)"), "the replay yields to the seat's slots");
    }
}
