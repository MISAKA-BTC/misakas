//! **ADR-0152 v3.1 Phase 2, P2-8b + P2-8d — the replay filer** (node policy; a child module of
//! `palw_panel`, whose loop holds its one book and calls [`PalwPanelService::replay_filer_tick_v1`]
//! once a tick).
//!
//! **What it closes** (§3.9's garbage row, SR-8, X10, J-6, DA-3, DA-9; `phase2-plan.md` F14, §5.7).
//! A seat whose own replay of a claim's job does not reproduce the claim's committed roots — the
//! SEAT-R replay's `Refuted` (both lanes, `replay_refuted`), or a fault the interval arm, the segment
//! resume or the capture arm recorded (`seat_found_fault_v1`) — knew the producer lied and could file
//! nothing but a silence: `Refuted` yields roots, the interval arm a leaf or a block, the resume a
//! bool. After X10 an unfiled garbage claim costs 0, so this is required (§7.3), not optional:
//!
//! * **P2-8b**: the claim's served capture (the pool, or the copy this seat retained once it verified
//!   against the claim's roots) is bisected against this seat's own dense replay of the claim's job
//!   ([`palw_replay_contradiction_v1`], the ONE builder the real-claim suite also runs), and the
//!   first divergent step becomes `StepArithmetic` → `ExecutorRefuted` (kind 4), or — step trees that
//!   agree — `LogitsNotStepOutput` (12, F1c). Filed through [`replay_file_seam_v1`].
//! * **P2-8d**: a divergent leaf located on the claim's own capture that the served material cannot
//!   open is DEMANDED: a `DefaultAccusedHeld` naming `StepLeaf { leaf }` (DA-3: free past
//!   `palw_rcore_plus`, keyed by the session), asked of the chain through P2-6's own read
//!   (`palw_da_accusation_check_v1`, C-8) and its step rule ([`palw_seat_accuse_step_v1`]), queued on
//!   the court queue under its own key. The producer discloses the leaf's evidence or defaults (S1).
//! * **P2-8e** (held dissections of fused-attention leaves) is ONE hook,
//!   [`palw_replay_held_dissection_hook_v1`]: it waits for the audit's A-held line.
//!
//! **Resources.** Every run is off the panel's loop (a blocking task) under the ledger: the job's
//! reservation is the dense capture's need (`whole_capture_memory_need_v1`, the capture arm's figure:
//! a class whose capture cannot be laid out whole is the streamed routes' and is not run here), held
//! for the run's life, and every bisection rung reserves its own two prefix reads before it runs
//! ([`palw_replay_rung_bytes_v1`]). One run is in flight, at most [`PALW_REPLAY_FILER_STARTS_PER_TICK_V1`]
//! starts a tick, at most [`PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1`] runs a claim, and a bisection spends
//! at most `1 + ⌈log₂ n⌉` rungs (`palw_replay_bisect_rungs_v1`). A start the ledger refuses waits for a
//! later tick without spending the claim's run, as SEAT-R's replays do.
//!
//! **Filing is deduplicated.** One case a claim (the first trigger wins), one kind-4 offence a claim
//! (its ledger key is the court queue's key), one demand a claim; a carrier lost while the claim's
//! duty stands is queued once more, never a third time; nothing is filed against this node's own bond.
//!
//! **Liveness.** An honest producer is never accused: kind 4 is filed only when the fold's own
//! predicate convicts the proof (the builder's rule), and a demand only for a leaf located on the
//! claim's own verified capture that it could not open — never after a proof was built and held.
//! A seat whose backend or class differs finds no verified capture, or a local run of another job,
//! and abstains.
//!
//! **Consensus-inert, and dormant below `palw_rcore_plus`** ([`palw_replay_filer_armed_v1`]): nothing
//! is noted, run or filed there, and a node that carries nothing (no `--palw-fee-outpoint`) keeps no
//! book.

use super::*;
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwOffenceTargetV1};
use kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2;
use kaspa_consensus_core::palw_replay_refute_v1::{
    PalwReplayClaimV1, PalwReplayFindingV1, palw_replay_bisect_rungs_v1, palw_replay_contradiction_v1,
    palw_replay_executor_refuted_object_v1,
};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;

/// Runs a tick starts: one. A run is a whole dense replay of a claim's job, so the filer never
/// competes with the seat's own SEAT-R replays for more than one of the ledger's grants at a time.
pub(super) const PALW_REPLAY_FILER_STARTS_PER_TICK_V1: usize = 1;
/// Cases a tick tries to start before it gives the slot up — each one that waits is tried again at
/// the next DAA, so these are distinct cases.
pub(super) const PALW_REPLAY_FILER_TRIES_PER_TICK_V1: usize = 4;
/// Runs one claim gets: the run, and one more after a failure that was this host's (a task that
/// did not finish, a local replay that did not run). A run's finding stands: a replay that found
/// nothing will find nothing again.
pub(super) const PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1: u8 = 2;
/// Claims the book pursues at once; a new trigger past it is dropped (it re-notes next tick if its
/// duty still stands) rather than evicting a case in flight.
pub(super) const PALW_REPLAY_FILER_CASES_MAX_V1: usize = 128;
/// How long a noted case is pursued, from its note: `window_court` (3,000 DAA on testnet-12), the
/// longest a kind 4 or a demand can still act on the claim's stage (DA-8's post-Final window).
pub(super) const PALW_REPLAY_FILER_CASE_DAA_V1: u64 = 3_000;
/// Carriers one filing may take: the first, and one more when the first was lost while the claim's
/// duty still stands (a conviction ends the duty; a claim licensed by others is the court's).
pub(super) const PALW_REPLAY_FILER_SENDS_V1: u8 = 2;
/// When a sent filing counts as lost: six re-plans with the duty still standing.
pub(super) const PALW_REPLAY_FILER_REFILE_DAA_V1: u64 = 6 * COURT_MOVE_REPLAN_DAA;
/// The court queue's round of a kind-4 filing — keyed by its offence key
/// ([`palw_replay_refuted_queue_key_v1`]), a round no court move, DA accusation (`u32::MAX`) or answer
/// (a folded unit digest) is keyed by.
pub(super) const PALW_REPLAY_REFUTED_QUEUE_ROUND_V1: u32 = u32::MAX - 1;
/// The court queue's round of a P2-8d `StepLeaf` demand ([`palw_replay_demand_queue_key_v1`]).
pub(super) const PALW_REPLAY_DEMAND_QUEUE_ROUND_V1: u32 = u32::MAX - 2;

/// **Is the filer armed?** Past `palw_rcore_plus` (testnet-12 from genesis), on a node that carries
/// (`--palw-fee-outpoint`). Below the fence nothing is noted, run or filed — the fence-off twin.
pub(super) fn palw_replay_filer_armed_v1(rcore_plus: bool, carries: bool) -> bool {
    rcore_plus && carries
}

/// Where the seat's check found the mismatch that triggered a case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwReplayMismatchSiteV1 {
    /// The SEAT-R replay did not reproduce the roots (`replay_refuted`, both lanes).
    Replay,
    /// A fault finder recorded one (`seat_found_fault_v1`: the interval arm, the segment resume, the
    /// capture arm).
    FaultFinder,
}

/// **What a filing hands the reporter filer** (P2-8's commit–reveal, at integration): the claim, the
/// offence's ledger key (one kind 4 a claim), the evidence id R-3's commitment binds (N12), the
/// accused (the claim's executor, never this node's bond), and the object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PalwReplayFilingV1 {
    pub claim_id: Hash64,
    pub offence_key: Hash64,
    pub evidence_id: Hash64,
    pub accused: PalwBondKeyV2,
    pub object: PalwConsensusObjectV2,
}

/// The court queue's key of a kind-4 filing: its offence key — so a second filer of the same offence
/// (the capture arm's P2-8) queues under the same key and the queue holds one.
pub(super) fn palw_replay_refuted_queue_key_v1(offence_key: Hash64) -> (Hash64, u32, bool) {
    (offence_key, PALW_REPLAY_REFUTED_QUEUE_ROUND_V1, false)
}

/// The court queue's key of this seat's P2-8d demand on `claim` — one a claim.
pub(super) fn palw_replay_demand_queue_key_v1(claim: Hash64) -> (Hash64, u32, bool) {
    (claim, PALW_REPLAY_DEMAND_QUEUE_ROUND_V1, false)
}

/// **The one seam P2-8b's `ExecutorRefuted` leaves through** — onto the panel's court queue, drained
/// by `carry_priority_v1` on the priority lane of P2-6's one scheduler (`PalwCarrierSlotsV1`), under
/// the offence's own key; `false` (nothing queued) when that key is queued already.
///
/// P2-8 seam: routed through the reporter filer (PalwConvictionFilingV1) at integration.
///
/// Until then the object rides bare, reporter slot empty — a conviction without a commitment pays no
/// reporter (R-3), which P2-8's filer adds by committing over `(offence_key, evidence_id, reporter)`,
/// waiting for acceptance, then filing and revealing. What it needs is on the filing: the offence
/// key, the evidence id and the accused.
pub(super) fn replay_file_seam_v1(
    court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    filing: &PalwReplayFilingV1,
) -> bool {
    let key = palw_replay_refuted_queue_key_v1(filing.offence_key);
    if court_pending.iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key) {
        return false;
    }
    court_pending.push((key.0, key.1, key.2, filing.object.clone()));
    true
}

/// **P2-8e's one hook: a divergent fused-attention leaf** (§3.9, DA-3: `DaUnitNeedsDissection`). Its
/// terminal is a HELD dissection — the audit's A-held line (C1–C5, `CourtAttnRootClaimedHeld`, tag
/// 57) — which is not built here: the case settles, said once, and files nothing (a named fused leaf
/// is refused as a DA unit, and no one-step proof opens it). When A-held lands, this is where the
/// held dissection's opening move is queued.
pub(super) fn palw_replay_held_dissection_hook_v1(claim: &Hash64, leaf: u64, _binding: &PalwStepBindingV2) {
    warn!(
        "[{PALW_PANEL}] claim {claim}: the first divergent step is fused-attention leaf {leaf} — a held dissection's (P2-8e, \
         waiting for the audit's A-held line); nothing filed from the replay"
    );
}

/// **The bytes one bisection rung reads**: both captures' prefix commitments at one index — each
/// decodes its capture and lays out its leaf hashes (`bisect_prefix_state`), so twice the capture and
/// twice `n` 64-byte leaves. Reserved on the ledger before the rung runs.
pub(super) fn palw_replay_rung_bytes_v1(capture_bytes: u64, step_leaf_count: u64) -> u64 {
    capture_bytes.saturating_add(step_leaf_count.saturating_mul(64)).saturating_mul(2)
}

/// One served capture the run may use, with the roots it must reproduce and the job the seat's own
/// replay runs (the claim's: the anchor's on the attempt lane, the verified payload's own on the
/// free-prompt lane).
pub(super) struct PalwReplayServedV1 {
    pub capture: Vec<u8>,
    pub roots: PalwClaimRootsV1,
    /// The job's ids on the free-prompt lane.
    pub prompt_token_ids: Option<Vec<u32>>,
    pub work: ReplayWork,
}

/// **One run, off the loop**: the first candidate whose capture reproduces the claim's roots (else
/// the first carrying the claim's binding, F1c's), this seat's own dense replay of its job, and the
/// builder — each bisection rung reserved on `ledger`
/// (`role: "replay-bisect"`) for its two reads. `Err` is this host's failure (the local replay did
/// not run); every finding about the claim is `Ok`.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_replay_filer_job_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    candidates: Vec<PalwReplayServedV1>,
    target: &PalwOffenceTargetV1,
    ladder: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
) -> Result<PalwReplayFindingV1, String> {
    use kaspa_consensus_core::palw_replay_refute_v1::PalwReplayNothingV1;
    // A capture that verifies first; else one that carries the claim's own binding — the garbage-logits
    // producer's, which no seat rule verifies and whose proof (12) the builder authenticates against the
    // claim's roots. Asked BEFORE the local replay is paid for.
    let carries_the_claims_binding = |capture: &[u8]| {
        backend
            .disclose_trace_event(capture, u32::MAX, u8::MAX)
            .is_ok_and(|disclosure| disclosure.binding().committed_execution_root == target.execution_root)
    };
    let pick = candidates
        .iter()
        .position(|c| backend.verify_material(&c.capture, c.roots) == PalwMaterialVerdictV1::Matches)
        .or_else(|| candidates.iter().position(|c| carries_the_claims_binding(&c.capture)));
    let Some(served) = pick.and_then(|pick| candidates.into_iter().nth(pick)) else {
        return Ok(PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotTheClaims, rungs: 0 });
    };
    let local = served.work.run(backend).ok_or("this seat's own replay of the claim's job did not run")?.material;
    let n = backend.capture_shape(&served.capture).map(|shape| shape.step_leaf_count).unwrap_or(0);
    let rung_bytes = palw_replay_rung_bytes_v1(served.capture.len() as u64, n);
    let key = crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
        role: "replay-bisect",
        class_id: target.class_id,
        job: target.claim_id,
    };
    Ok(palw_replay_contradiction_v1(
        backend,
        &served.capture,
        &local,
        PalwReplayClaimV1 { target, roots: served.roots, ladder, form, prompt_token_ids: served.prompt_token_ids.as_deref() },
        palw_replay_bisect_rungs_v1(n),
        |_rung| ledger.reserve(key.clone(), rung_bytes).map_err(|refusal| refusal.to_string()),
    ))
}

/// **The claim as kind 4's adjudicator resolves it**, from the seat duty's copies of the claim record
/// (`palw_offence_target_v1` reads the same fields off state): what the builder's predicates read.
pub(super) fn palw_replay_target_of_duty_v1(duty: &PalwSeatDutyV2) -> PalwOffenceTargetV1 {
    PalwOffenceTargetV1 {
        claim_id: duty.claim_id,
        class_id: duty.class_id,
        artifact_root: duty.artifact_root,
        executor_bond: duty.executor_bond,
        execution_root: duty.execution_root,
        lane: Some(if duty.free_prompt { PalwClaimSourceKindV1::FreePrompt } else { PalwClaimSourceKindV1::Attempt }),
        segment_count: Some(kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(duty.panel_seat_count.max(1))),
        phase: None,
        job_identity: duty.job_identity,
        trace_root: duty.trace_root,
        output_root: duty.output_root,
    }
}

/// Where one case stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwReplayCaseStepV1 {
    /// Noted; the builder has not run, or ran into this host's failure and may run again.
    Waiting,
    /// The one run in flight.
    Running,
    /// Kind 4 handed to the seam, `sends` times.
    Filed { filing: Box<PalwReplayFilingV1>, sends: u8 },
    /// P2-8d: the `StepLeaf` demand, asked of the chain until it lands or settles. `sends` carriers
    /// queued; `refused_at` the DAA the chain last refused it for A-6's room.
    Demand { leaf: u64, binding: Box<PalwStepBindingV2>, sends: u8, refused_at: Option<u64> },
    /// Nothing more to do.
    Settled,
}

/// One claim this seat pursues.
#[derive(Clone, Debug)]
pub(super) struct PalwReplayCaseV1 {
    pub duty: PalwSeatDutyV2,
    pub site: PalwReplayMismatchSiteV1,
    pub noted_daa: u64,
    pub runs: u8,
    /// The DAA before which a start that had to wait is not tried again ([`PalwReplayFilerV1::due_v1`]):
    /// a case waiting on a served capture or on the ledger never holds the others behind it.
    pub retry_at: u64,
    pub step: PalwReplayCaseStepV1,
}

/// **The filer's book** — the panel loop holds one. Cases by claim, and the one run in flight.
#[derive(Default)]
pub(super) struct PalwReplayFilerV1 {
    cases: BTreeMap<Hash64, PalwReplayCaseV1>,
    running: Option<(Hash64, tokio::task::JoinHandle<Result<PalwReplayFindingV1, String>>)>,
}

impl PalwReplayFilerV1 {
    /// **Note a mismatch on `duty`'s claim.** `false` when nothing was noted: the claim is this
    /// node's own (never filed against its own bond), a case for it exists (the first trigger wins,
    /// and a settled case is not re-opened by the same trigger on the next tick), or the book is full.
    pub(super) fn note_v1(
        &mut self,
        duty: &PalwSeatDutyV2,
        site: PalwReplayMismatchSiteV1,
        current_daa: u64,
        own_bond: &PalwBondKeyV2,
    ) -> bool {
        if duty.executor_bond == *own_bond || self.cases.contains_key(&duty.claim_id) {
            return false;
        }
        if self.cases.len() >= PALW_REPLAY_FILER_CASES_MAX_V1 {
            let settled = self.cases.iter().find(|(_, case)| case.step == PalwReplayCaseStepV1::Settled).map(|(claim, _)| *claim);
            match settled {
                Some(claim) => {
                    self.cases.remove(&claim);
                }
                None => return false,
            }
        }
        self.cases.insert(
            duty.claim_id,
            PalwReplayCaseV1 {
                duty: duty.clone(),
                site,
                noted_daa: current_daa,
                runs: 0,
                retry_at: current_daa,
                step: PalwReplayCaseStepV1::Waiting,
            },
        );
        true
    }

    /// Forget cases older than [`PALW_REPLAY_FILER_CASE_DAA_V1`] — never the one in flight, whose
    /// task still holds its reservation until it returns. Returns the court-queue keys the forgotten
    /// cases were debounced under (its demand's, and its filing's), for `court_moved` to drop.
    pub(super) fn expire_v1(&mut self, current_daa: u64) -> Vec<(Hash64, u32, bool)> {
        let running = self.running.as_ref().map(|(claim, _)| *claim);
        let old: Vec<Hash64> = self
            .cases
            .iter()
            .filter(|(claim, case)| {
                Some(**claim) != running && current_daa > case.noted_daa.saturating_add(PALW_REPLAY_FILER_CASE_DAA_V1)
            })
            .map(|(claim, _)| *claim)
            .collect();
        let mut keys = Vec::new();
        for claim in &old {
            if let Some(case) = self.cases.remove(claim) {
                keys.push(palw_replay_demand_queue_key_v1(*claim));
                if let PalwReplayCaseStepV1::Filed { filing, .. } = case.step {
                    keys.push(palw_replay_refuted_queue_key_v1(filing.offence_key));
                }
            }
        }
        keys
    }

    /// **The next case to run at `current_daa`** — none while one is in flight; else the oldest
    /// waiting case with a run left whose last wait has passed (`retry_at`).
    pub(super) fn due_v1(&self, current_daa: u64) -> Option<Hash64> {
        if self.running.is_some() {
            return None;
        }
        self.cases
            .iter()
            .filter(|(_, case)| {
                case.step == PalwReplayCaseStepV1::Waiting
                    && case.runs < PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1
                    && case.retry_at <= current_daa
            })
            .min_by_key(|(claim, case)| (case.noted_daa, **claim))
            .map(|(claim, _)| *claim)
    }

    /// A start on `claim` had to wait: it is not tried again before the next DAA, so the cases behind
    /// it are.
    fn wait(&mut self, claim: &Hash64, current_daa: u64) {
        if let Some(case) = self.cases.get_mut(claim) {
            case.retry_at = current_daa.saturating_add(1);
        }
    }

    pub(super) fn case(&self, claim: &Hash64) -> Option<&PalwReplayCaseV1> {
        self.cases.get(claim)
    }

    fn settle(&mut self, claim: &Hash64) {
        if let Some(case) = self.cases.get_mut(claim) {
            case.step = PalwReplayCaseStepV1::Settled;
        }
    }

    /// A run started on `claim`: its run is spent.
    fn started(&mut self, claim: Hash64, handle: tokio::task::JoinHandle<Result<PalwReplayFindingV1, String>>) {
        if let Some(case) = self.cases.get_mut(&claim) {
            case.runs += 1;
            case.step = PalwReplayCaseStepV1::Running;
        }
        self.running = Some((claim, handle));
    }

    /// **What a finished run makes of its case** — the pure half of [`PalwPanelService::replay_filer_tick_v1`]'s
    /// poll. `Refutes` becomes a filing through the seam (kind 4 built by
    /// `palw_replay_executor_refuted_object_v1`, the gate's own encoding); `DemandLeaf` a P2-8d demand;
    /// `NeedsDissection` P2-8e's hook; `Nothing` settles; a host failure waits for the claim's next run
    /// (or settles when none is left). Returns the filing queued now, if any.
    pub(super) fn on_finding_v1(
        &mut self,
        claim: Hash64,
        outcome: Result<PalwReplayFindingV1, String>,
        own_bond: &PalwBondKeyV2,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    ) -> Option<PalwReplayFilingV1> {
        let case = self.cases.get_mut(&claim)?;
        match outcome {
            Ok(PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs }) => {
                let executor = case.duty.executor_bond;
                if executor == *own_bond {
                    case.step = PalwReplayCaseStepV1::Settled;
                    return None;
                }
                match palw_replay_executor_refuted_object_v1(claim, executor, contradiction, prompt_ids_opening) {
                    Ok(built) => {
                        let filing = PalwReplayFilingV1 {
                            claim_id: claim,
                            offence_key: built.offence_id,
                            evidence_id: built.evidence_id,
                            accused: executor,
                            object: built.object,
                        };
                        info!(
                            "[{PALW_PANEL}] claim {claim}: this seat's replay refutes its executor at {site:?} ({rungs} bisection rungs, \
                             found by its {:?} check) — filing ExecutorRefuted (offence {}, evidence {}; ADR-0152 J-4, SR-8, P2-8b)",
                            case.site, filing.offence_key, filing.evidence_id
                        );
                        let queued = replay_file_seam_v1(court_pending, &filing);
                        case.step = PalwReplayCaseStepV1::Filed { filing: Box::new(filing.clone()), sends: u8::from(queued) };
                        queued.then_some(filing)
                    }
                    Err(why) => {
                        warn!("[{PALW_PANEL}] claim {claim}: the refutation at {site:?} cannot be filed: {why}");
                        case.step = PalwReplayCaseStepV1::Settled;
                        None
                    }
                }
            }
            Ok(PalwReplayFindingV1::DemandLeaf { leaf, binding, why, rungs }) => {
                info!(
                    "[{PALW_PANEL}] claim {claim}: the first divergent step is leaf {leaf} ({rungs} bisection rungs) and {why} — \
                     demanding it in a data-availability session (ADR-0152 DA-3, J-6, P2-8d)"
                );
                case.step = PalwReplayCaseStepV1::Demand { leaf, binding, sends: 0, refused_at: None };
                None
            }
            Ok(PalwReplayFindingV1::NeedsDissection { leaf, binding, .. }) => {
                palw_replay_held_dissection_hook_v1(&claim, leaf, &binding);
                case.step = PalwReplayCaseStepV1::Settled;
                None
            }
            Ok(PalwReplayFindingV1::Nothing { why, rungs }) => {
                info!("[{PALW_PANEL}] claim {claim}: the replay filer files nothing ({why:?}, {rungs} bisection rungs)");
                case.step = PalwReplayCaseStepV1::Settled;
                None
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the replay filer's run failed on this host: {why}");
                case.step = if case.runs < PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1 {
                    PalwReplayCaseStepV1::Waiting
                } else {
                    PalwReplayCaseStepV1::Settled
                };
                None
            }
        }
    }

    /// **Filings whose carrier was lost** — sent once, not queued, their send older than
    /// [`PALW_REPLAY_FILER_REFILE_DAA_V1`], the claim's duty still standing (a conviction voids the
    /// claim and ends it), and a send left: queued once more through the seam.
    pub(super) fn refile_lost_v1(
        &mut self,
        current_daa: u64,
        live_duty: impl Fn(&Hash64) -> bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) -> usize {
        let mut refiled = 0;
        for (claim, case) in self.cases.iter_mut() {
            let PalwReplayCaseStepV1::Filed { filing, sends } = &mut case.step else { continue };
            let key = palw_replay_refuted_queue_key_v1(filing.offence_key);
            let lost = court_moved.get(&key).is_some_and(|sent| current_daa >= sent.saturating_add(PALW_REPLAY_FILER_REFILE_DAA_V1));
            if *sends < PALW_REPLAY_FILER_SENDS_V1 && lost && live_duty(claim) && replay_file_seam_v1(court_pending, filing) {
                *sends += 1;
                refiled += 1;
            }
        }
        refiled
    }

    /// The P2-8d demands the chain is asked about now: not queued, not sent or refused for room less
    /// than a re-plan ago, a send left.
    pub(super) fn demands_due_v1(
        &self,
        current_daa: u64,
        court_pending: &[(Hash64, u32, bool, PalwConsensusObjectV2)],
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) -> Vec<Hash64> {
        let fresh = |at: Option<u64>| at.is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA));
        self.cases
            .iter()
            .filter(|(claim, case)| {
                let PalwReplayCaseStepV1::Demand { sends, refused_at, .. } = &case.step else { return false };
                let key = palw_replay_demand_queue_key_v1(**claim);
                *sends < PALW_REPLAY_FILER_SENDS_V1
                    && !court_pending.iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key)
                    && !fresh(court_moved.get(&key).copied())
                    && !fresh(*refused_at)
            })
            .map(|(claim, _)| *claim)
            .collect()
    }
}

impl PalwPanelService {
    /// **P2-8b / P2-8d, once a tick** — the filer's one call site in the panel loop.
    ///
    /// 1. Notes this tick's triggers on live duties: a SEAT-R replay that refuted (`replay_refuted`)
    ///    and a fault a fault finder recorded (`seat_found_fault_v1`).
    /// 2. Polls the run in flight and files what it found ([`PalwReplayFilerV1::on_finding_v1`]);
    ///    queues once more a filing whose carrier was lost.
    /// 3. Asks the chain about each P2-8d demand (P2-6's `palw_da_accusation_check_v1`, C-8, and its
    ///    step rule) and queues the ones it would open.
    /// 4. Starts at most [`PALW_REPLAY_FILER_STARTS_PER_TICK_V1`] run, off the loop, under the ledger.
    ///
    /// Below `palw_rcore_plus`, or on a node that carries nothing, the book is emptied and nothing
    /// runs ([`palw_replay_filer_armed_v1`]).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn replay_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReplayFilerV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        duties: &[PalwSeatDutyV2],
        replay_refuted: &HashSet<Hash64>,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        if !palw_replay_filer_armed_v1(
            self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
            self.config.fee_outpoint.is_some(),
        ) {
            // A run in flight keeps its reservation until its task returns; the handle is dropped
            // (the task runs to its end detached) and its finding is not filed.
            *filer = PalwReplayFilerV1::default();
            return;
        }
        // 1. This tick's triggers.
        for duty in duties {
            let site = if replay_refuted.contains(&duty.claim_id) {
                PalwReplayMismatchSiteV1::Replay
            } else if self.seat_found_fault_v1(&duty.claim_id) {
                PalwReplayMismatchSiteV1::FaultFinder
            } else {
                continue;
            };
            if filer.note_v1(duty, site, current_daa, &bond_key) {
                info!(
                    "[{PALW_PANEL}] claim {}: this seat's {site:?} check does not reproduce the claim — its replay filer will \
                     bisect the served capture against its own replay (ADR-0152 SR-8, P2-8b)",
                    duty.claim_id
                );
            }
        }
        for key in filer.expire_v1(current_daa) {
            court_moved.remove(&key);
        }
        // 2. The run in flight.
        if filer.running.as_ref().is_some_and(|(_, handle)| handle.is_finished()) {
            let (claim, handle) = filer.running.take().expect("checked above");
            let outcome = handle.await.unwrap_or_else(|e| Err(format!("the replay filer's task did not finish: {e}")));
            filer.on_finding_v1(claim, outcome, &bond_key, court_pending);
        }
        let live: HashSet<Hash64> = duties.iter().map(|duty| duty.claim_id).collect();
        let refiled = filer.refile_lost_v1(current_daa, |claim| live.contains(claim), court_pending, court_moved);
        if refiled > 0 {
            info!(
                "[{PALW_PANEL}] queued {refiled} ExecutorRefuted filing(s) again: the carrier was lost while the duty stands (P2-8b)"
            );
        }
        // 3. P2-8d's demands, through P2-6's read of the chain.
        for claim in filer.demands_due_v1(current_daa, court_pending, court_moved) {
            self.replay_demand_v1(session, filer, claim, current_daa, network_domain, bond_key, court_pending);
        }
        // 4. The next run: at most one start, over at most a few tries, so a case that waits (no
        // served capture yet, the ledger full) never holds the ones behind it.
        let mut started = 0;
        for _ in 0..PALW_REPLAY_FILER_TRIES_PER_TICK_V1 {
            if started >= PALW_REPLAY_FILER_STARTS_PER_TICK_V1 {
                break;
            }
            let Some(claim) = filer.due_v1(current_daa) else { break };
            let duty = filer.case(&claim).expect("a due case").duty.clone();
            match self.replay_filer_start_v1(session, &duty, network_domain, materials) {
                Ok(handle) => {
                    filer.started(claim, handle);
                    started += 1;
                }
                // The ledger, the claim's block, no served capture yet: tried again at the next DAA,
                // the run not spent.
                Err(PalwReplayStartV1::Wait(why)) => {
                    crate::palw_backends::note_throttled_v1("panel-replay-filer-wait", || {
                        format!("[{PALW_PANEL}] claim {claim}: the replay filer's run waits — {why}")
                    });
                    filer.wait(&claim, current_daa);
                }
                Err(PalwReplayStartV1::Never(why)) => {
                    info!("[{PALW_PANEL}] claim {claim}: the replay filer does not run — {why}");
                    filer.settle(&claim);
                }
            }
        }
    }

    /// **One P2-8d demand, asked of the chain and queued** — P2-6's read (`palw_da_accusation_check_v1`:
    /// the fold's C-8 gate, `AccusedBefore` once this seat's session exists or existed) and its step
    /// rule (`palw_seat_accuse_step_v1`: only A-6's room is waited out), then the ONE held builder
    /// (`palw_da_held_accusation_object_v1`, which asks the fold's stateless halves of the unit first).
    ///
    /// **`Answered`** is P2-6's read of ITS named unit, row 0 — not this `StepLeaf` — and it stops that
    /// read before C-8. On the garbage path it is the usual answer (the producer answers DA), so it is
    /// not a refusal of the demand: the demand is filed on it, at most [`PALW_REPLAY_FILER_SENDS_V1`]
    /// times, and the fold decides C-8 (the residual: a demand the fold refuses for the accuser's room
    /// costs one carrier's fee, never a charge).
    #[allow(clippy::too_many_arguments)]
    fn replay_demand_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReplayFilerV1,
        claim: Hash64,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    ) {
        use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 as C;
        let Some(check) = session.palw_da_accusation_check_v1(claim, bond_key) else { return };
        let file = match (&check, palw_seat_accuse_step_v1(&check)) {
            (C::Answered, _) => true,
            (_, PalwSeatAccuseStepV1::File) => true,
            (_, PalwSeatAccuseStepV1::Retry) => {
                if let Some(PalwReplayCaseV1 { step: PalwReplayCaseStepV1::Demand { refused_at, .. }, .. }) =
                    filer.cases.get_mut(&claim)
                {
                    *refused_at = Some(current_daa);
                }
                return;
            }
            (_, PalwSeatAccuseStepV1::Settle) => {
                info!("[{PALW_PANEL}] claim {claim}: the StepLeaf demand settles — {check:?} (P2-8d)");
                filer.settle(&claim);
                return;
            }
        };
        debug_assert!(file);
        let Some(case) = filer.cases.get_mut(&claim) else { return };
        let PalwReplayCaseStepV1::Demand { leaf, binding, sends, .. } = &mut case.step else { return };
        let (leaf, execution_root, class_id) = (*leaf, case.duty.execution_root, case.duty.class_id);
        match kaspa_consensus_core::palw_da_rcore_v1::palw_da_held_accusation_object_v1(
            &network_domain,
            claim,
            &execution_root,
            kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf },
            (**binding).clone(),
            bond_key,
            self.class_prompt_ids_form(class_id),
            |message, context| self.sign(message, context),
        ) {
            Ok(object) => {
                info!(
                    "[{PALW_PANEL}] claim {claim}: demanding step leaf {leaf}'s evidence in a data-availability session — {check:?} \
                     (ADR-0152 DA-3, J-6, P2-8d)"
                );
                let key = palw_replay_demand_queue_key_v1(claim);
                court_pending.push((key.0, key.1, key.2, object));
                *sends += 1;
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the StepLeaf demand at leaf {leaf} cannot be built: {why}");
                filer.settle(&claim);
            }
        }
    }

    /// **Start one run**: the class's backend, the served candidates (this seat's retained copy first,
    /// then the pool's), the job the seat replays, and the dense capture's reservation (the capture
    /// arm's `whole_capture_memory_need_v1`), held by the blocking task for its life.
    fn replay_filer_start_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &PalwSeatDutyV2,
        network_domain: Hash64,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
    ) -> Result<tokio::task::JoinHandle<Result<PalwReplayFindingV1, String>>, PalwReplayStartV1> {
        let backend = self
            .resolve_backend(session, duty.class_id, duty.artifact_root)
            .map_err(|e| PalwReplayStartV1::Never(format!("the class: {e}")))?;
        let form = self.class_prompt_ids_form(duty.class_id);
        let ladder = self.class_step_ladder(duty.class_id);
        let retained = std::fs::read(self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id))).ok();
        let payloads: Vec<Vec<u8>> =
            retained.into_iter().chain(materials.get(&duty.claim_id).into_iter().flatten().cloned()).collect();
        let candidates: Vec<PalwReplayServedV1> = if duty.free_prompt {
            payloads
                .iter()
                .filter_map(|bytes| kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(bytes, form))
                .filter(|payload| {
                    payload.material.job.class_id == duty.class_id && payload.material.job.executor_bond == duty.executor_bond.0
                })
                .map(|payload| {
                    let job = payload.material.job;
                    let prompt: Vec<usize> = payload.material.prompt_token_ids.iter().map(|t| *t as usize).collect();
                    PalwReplayServedV1 {
                        capture: payload.capture,
                        roots: PalwClaimRootsV1 {
                            execution_root: duty.execution_root,
                            trace_root: duty.trace_root,
                            anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job),
                            attempt_draw: None,
                            output_root: Some(duty.output_root),
                            job_pin: duty.fp_job_pin_v1(),
                        },
                        prompt_token_ids: Some(payload.material.prompt_token_ids),
                        work: ReplayWork::FreePrompt(job, prompt),
                    }
                })
                .collect()
        } else {
            let (ctx, prompt) = self
                .attempt_job_for_claim(
                    session,
                    backend.as_ref(),
                    network_domain,
                    duty.accepted_block,
                    duty.class_id,
                    &duty.executor_bond,
                )
                .ok_or_else(|| PalwReplayStartV1::Wait("the claim's block is not in this node's store".into()))?;
            let anchor = self
                .job_anchor_for_claim(
                    session,
                    backend.as_ref(),
                    network_domain,
                    duty.accepted_block,
                    duty.class_id,
                    &duty.executor_bond,
                )
                .unwrap_or_default();
            let roots = PalwClaimRootsV1 {
                execution_root: duty.execution_root,
                trace_root: duty.trace_root,
                anchor,
                attempt_draw: self.attempt_draw_for_claim(session, duty.accepted_block),
                output_root: Some(duty.output_root),
                job_pin: duty.fp_job_pin_v1(),
            };
            payloads
                .into_iter()
                .map(|capture| PalwReplayServedV1 {
                    capture,
                    roots,
                    prompt_token_ids: None,
                    work: ReplayWork::Attempt(ctx.clone(), prompt.clone()),
                })
                .collect()
        };
        let Some(first) = candidates.first() else {
            // Nothing served that could be the claim's: the SEAT-R replay refuted a claim whose
            // producer served this seat nothing, which is P2-6's accusation, not a proof.
            return Err(PalwReplayStartV1::Wait("no served capture of the claim is held".into()));
        };
        let need = self
            .backends()
            .whole_capture_memory_need_v1(backend.as_ref(), duty.class_id, duty.artifact_root, &first.capture, ladder, |id| {
                self.chain_carriage_v1(session, id)
            })
            .map_err(|why| PalwReplayStartV1::Never(format!("the served capture is not laid out whole on this seat ({why})")))?;
        let reserved = self.reserve_replay_v1("replay-filer", &need, duty.class_id, duty.claim_id).map_err(PalwReplayStartV1::Wait)?;
        let target = palw_replay_target_of_duty_v1(duty);
        info!(
            "[{PALW_PANEL}] claim {}: bisecting the served capture against this seat's own replay off the loop ({} held, P2-8b)",
            duty.claim_id,
            need.describe()
        );
        Ok(tokio::task::spawn_blocking(move || {
            let _held_for_the_run = reserved;
            palw_replay_filer_job_v1(backend.as_ref(), candidates, &target, ladder, form, &crate::palw_memory_ledger::host_ledger_v1())
        }))
    }
}

/// Why a run did not start this tick.
#[derive(Debug)]
pub(super) enum PalwReplayStartV1 {
    /// Asked again next tick, the run not spent: the ledger, the claim's block, a served capture not
    /// yet held.
    Wait(String),
    /// Never on this seat: the class does not resolve, or its capture cannot be laid out whole.
    Never(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_offence_attribution_v1::PALW_FALSE_VALID_NETWORK_LADDER_V1;
    use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
    use kaspa_consensus_core::palw_replay_refute_v1::{PalwReplayNothingV1, PalwReplaySiteV1};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }

    /// A seat duty on `claim`, produced by `executor`.
    fn duty(claim: u64, executor: u64) -> PalwSeatDutyV2 {
        PalwSeatDutyV2 {
            accepted_block: h(0xB0 + claim),
            claim_id: h(claim),
            class_id: h(0xC1),
            artifact_root: h(0xA1),
            seat_bond: bond(1),
            executor_bond: bond(executor),
            execution_root: h(0xE0 + claim),
            trace_root: h(0x70 + claim),
            output_root: h(0x0070 + claim),
            bound_daa: 100,
            receipt_deadline: 700,
            panel_anchor: h(0xAC),
            seat_index: 1,
            panel_seat_count: 5,
            pwu: 1,
            quanta: 0,
            free_prompt: false,
            work_leaves: 0,
            job_identity: h(0x1D),
        }
    }

    /// The floor as testnet-12 registers it: its class, its pinned artifact root, the network's
    /// Merkle prompt carriage.
    fn floor() -> (misaka_palw_base0::backend::Base0Backend, Hash64, Hash64) {
        use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
        let backend =
            misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
                .with_step_ladder_cap(court.max_step_leaf_count())
                .with_prompt_ids_form(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1);
        (backend, entry.class_id(), root)
    }

    /// A real floor attempt job, the honest run, and — when `lie` — the drill's one-lane lie at the
    /// first openable leaf from the middle of the step space on, re-committed (a self-consistent
    /// garbage trace only a replay finds). Returns the job, the claim's capture, its roots, the
    /// target a node builds from its duty, and the faulted leaf.
    #[allow(clippy::type_complexity)]
    fn claim(
        backend: &misaka_palw_base0::backend::Base0Backend,
        class_id: Hash64,
        artifact_root: Hash64,
        lie: bool,
    ) -> (ReplayWork, Vec<u8>, PalwClaimRootsV1, PalwOffenceTargetV1, Option<u64>) {
        let anchor = h(0x5EED_0001);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the floor derives a job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(canonical, true);
        let honest = backend.execute(&job, &prompt).expect("the floor runs");
        let (run, leaf) = if lie {
            let shape = backend.capture_shape(&honest.material).expect("a capture");
            let leaf = (shape.step_leaf_count / 2..shape.step_leaf_count)
                .find(|leaf| backend.refutation_for_index(&honest.material, *leaf).is_ok())
                .expect("an openable leaf");
            (backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the lie runs"), Some(leaf))
        } else {
            (honest, None)
        };
        let roots = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor,
            attempt_draw: Some(true),
            output_root: Some(run.output_root),
            job_pin: None,
        };
        let mut d = duty(0x42, 7);
        (d.class_id, d.artifact_root, d.execution_root, d.trace_root, d.output_root) =
            (class_id, artifact_root, run.execution_root, run.trace_root, run.output_root);
        (ReplayWork::Attempt(job, prompt), run.material, roots, palw_replay_target_of_duty_v1(&d), leaf)
    }

    fn ledger() -> Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1> {
        crate::palw_memory_ledger::PalwMemoryLedgerV1::new(crate::palw_memory_ledger::PalwMemoryPoolV1::Host, Some(1 << 40), || None)
    }

    /// **T54f (node half), P2-8b on a real floor run**: the seat's own replay bisects the served lying
    /// capture to the injected leaf, in at most `1 + ⌈log₂ n⌉` rungs each reserved on the ledger
    /// (the O(log n) bound on a real trace) and released with its rung, and the proof it builds is
    /// kind 4's `StepArithmetic` that the fold's own predicate convicts; the object encodes it as the
    /// gate decodes it, under the per-claim ledger key, and the seam queues it once under that key.
    #[test]
    fn p2_8b_a_real_lie_is_bisected_to_its_leaf_in_log_rungs_and_filed_once() {
        let (backend, class_id, artifact_root) = floor();
        let (work, capture, roots, target, leaf) = claim(&backend, class_id, artifact_root, true);
        let leaf = leaf.unwrap();
        let n = backend.capture_shape(&capture).unwrap().step_leaf_count;
        let ledger = ledger();
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![PalwReplayServedV1 { capture: capture.clone(), roots, prompt_token_ids: None, work }],
            &target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            &ledger,
        )
        .expect("the local replay runs");
        let PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs } = finding else {
            panic!("the lie is refuted: {finding:?}")
        };
        assert_eq!(site, PalwReplaySiteV1::StepLeaf(leaf), "the first divergent step is the injected one");
        assert!(rungs <= palw_replay_bisect_rungs_v1(n) && rungs >= 2, "{rungs} rungs over {n} leaves: O(log n)");
        assert_eq!(ledger.reserved_bytes(), 0, "every rung's reservation is released with its rung");
        assert!(matches!(contradiction, PalwPanelContradictionV1::StepArithmetic { .. }));
        kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_convicts_execution_v2(
            &contradiction,
            prompt_ids_opening.as_ref(),
            target.execution_root,
            target.artifact_root,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
        )
        .expect("the fold's own predicate convicts it");
        // The node's book: noted once, filed once through the seam, under the offence's own key.
        let d = PalwSeatDutyV2 { claim_id: target.claim_id, executor_bond: target.executor_bond, ..duty(0x42, 7) };
        let mut filer = PalwReplayFilerV1::default();
        assert!(filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 1_000, &bond(1)));
        assert!(!filer.note_v1(&d, PalwReplayMismatchSiteV1::FaultFinder, 1_001, &bond(1)), "one case a claim");
        let mut court_pending = Vec::new();
        let filing = filer
            .on_finding_v1(
                d.claim_id,
                Ok(PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs }),
                &bond(1),
                &mut court_pending,
            )
            .expect("filed");
        let offence =
            kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&d.executor_bond.0, &d.claim_id);
        assert_eq!((filing.offence_key, filing.accused), (offence, d.executor_bond));
        assert_eq!(court_pending.len(), 1);
        assert_eq!((court_pending[0].0, court_pending[0].1, court_pending[0].2), palw_replay_refuted_queue_key_v1(offence));
        let PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence_id, evidence } = &court_pending[0].3 else {
            panic!("an ObjectiveOffence")
        };
        assert_eq!(
            (*kind, *accused, *evidence_id),
            (kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted, d.executor_bond, filing.evidence_id)
        );
        assert_eq!(*evidence_id, kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence));
        let decoded: kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1 =
            borsh::from_slice(evidence).expect("the gate decodes it");
        assert!(decoded.reporter_reveal.is_empty() && decoded.claim_id == d.claim_id, "the reporter slot stays empty (R-3)");
        assert!(!replay_file_seam_v1(&mut court_pending, &filing), "the seam queues one filing an offence");
        // The lost carrier is queued once more while the duty stands, never a third time.
        court_pending.clear();
        let mut moved = HashMap::new();
        moved.insert(palw_replay_refuted_queue_key_v1(offence), 2_000);
        assert_eq!(
            filer.refile_lost_v1(2_000 + PALW_REPLAY_FILER_REFILE_DAA_V1 - 1, |_| true, &mut court_pending, &moved),
            0,
            "not yet"
        );
        assert_eq!(
            filer.refile_lost_v1(2_000 + PALW_REPLAY_FILER_REFILE_DAA_V1, |_| false, &mut court_pending, &moved),
            0,
            "duty ended"
        );
        assert_eq!(filer.refile_lost_v1(2_000 + PALW_REPLAY_FILER_REFILE_DAA_V1, |_| true, &mut court_pending, &moved), 1);
        court_pending.clear();
        assert_eq!(filer.refile_lost_v1(9_000, |_| true, &mut court_pending, &moved), 0, "two sends at most");
    }

    /// **An honest claim produces no filing**: its capture reproduces the roots and so does this
    /// seat's replay — `LocalReproduces`, no rung spent, nothing queued; and a capture that is not the
    /// claim's (another run's roots) is not the committed execution — `ServedNotTheClaims`.
    #[test]
    fn p2_8b_an_honest_claim_files_nothing() {
        let (backend, class_id, artifact_root) = floor();
        let (work, capture, roots, target, _) = claim(&backend, class_id, artifact_root, false);
        let ledger = ledger();
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![PalwReplayServedV1 { capture: capture.clone(), roots, prompt_token_ids: None, work }],
            &target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            &ledger,
        )
        .unwrap();
        assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, rungs: 0 });
        let (work, _, _, _, _) = claim(&backend, class_id, artifact_root, false);
        let other = PalwClaimRootsV1 { execution_root: h(0xBAD), ..roots };
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![PalwReplayServedV1 { capture, roots: other, prompt_token_ids: None, work }],
            &PalwOffenceTargetV1 { execution_root: h(0xBAD), ..target },
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            &ledger,
        )
        .unwrap();
        assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotTheClaims, rungs: 0 });
        let mut filer = PalwReplayFilerV1::default();
        let d = duty(0x42, 7);
        assert!(filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 1_000, &bond(1)));
        let mut court_pending = Vec::new();
        assert!(filer.on_finding_v1(d.claim_id, Ok(finding), &bond(1), &mut court_pending).is_none());
        assert!(court_pending.is_empty() && filer.case(&d.claim_id).unwrap().step == PalwReplayCaseStepV1::Settled);
        assert_eq!(filer.due_v1(1_000), None, "a settled case is not run again");
    }

    /// **A rung the ledger refuses stops the bisection there** — nothing past it runs, nothing is
    /// filed, and the reservation is the rung's: a host whose ledger is full spends none of it.
    #[test]
    fn p2_8b_a_refused_rung_stops_the_bisection() {
        let (backend, class_id, artifact_root) = floor();
        let (work, capture, roots, target, _) = claim(&backend, class_id, artifact_root, true);
        let tiny =
            crate::palw_memory_ledger::PalwMemoryLedgerV1::new(crate::palw_memory_ledger::PalwMemoryPoolV1::Host, Some(1), || None);
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![PalwReplayServedV1 { capture, roots, prompt_token_ids: None, work }],
            &target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            &tiny,
        )
        .unwrap();
        assert!(
            matches!(
                finding,
                PalwReplayFindingV1::Nothing {
                    why: PalwReplayNothingV1::Bisect(
                        kaspa_consensus_core::palw_replay_refute_v1::PalwReplayBisectStopV1::RungRefused { rung: 0, .. }
                    ),
                    ..
                }
            ),
            "{finding:?}"
        );
        assert_eq!(tiny.reserved_bytes(), 0);
    }

    /// **The book: never this node's own claim, one case a claim, one run at a time, a run a claim
    /// spent on a host failure at most twice, and the fence-off twin.**
    #[test]
    fn p2_8b_the_book_is_bounded_and_deduplicated() {
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        assert!(!filer.note_v1(&duty(1, 1), PalwReplayMismatchSiteV1::Replay, 10, &own), "never against this node's own bond");
        assert!(filer.note_v1(&duty(2, 7), PalwReplayMismatchSiteV1::Replay, 10, &own));
        assert!(filer.note_v1(&duty(3, 7), PalwReplayMismatchSiteV1::FaultFinder, 11, &own));
        assert_eq!(filer.due_v1(11), Some(h(2)), "the oldest first");
        // A start that waits (no served capture yet, the ledger full) is tried again at the next DAA,
        // and the case behind it is tried meanwhile — no head-of-line block.
        filer.wait(&h(2), 11);
        assert_eq!(filer.due_v1(11), Some(h(3)), "the next case, while the first waits");
        assert_eq!(filer.due_v1(12), Some(h(2)), "the first again at the next DAA, its run unspent");
        assert_eq!(filer.case(&h(2)).unwrap().runs, 0);
        // A host failure keeps the case for its second run, then settles it.
        let mut court_pending = Vec::new();
        for (run, then) in [(1u8, PalwReplayCaseStepV1::Waiting), (2, PalwReplayCaseStepV1::Settled)] {
            filer.cases.get_mut(&h(2)).unwrap().runs = run;
            filer.on_finding_v1(h(2), Err("the task did not finish".into()), &own, &mut court_pending);
            assert_eq!(filer.case(&h(2)).unwrap().step, then, "after run {run}");
        }
        assert_eq!(filer.due_v1(12), Some(h(3)));
        // Expiry forgets a case after its window, and hands back the keys it was debounced under.
        assert_eq!(filer.expire_v1(11 + PALW_REPLAY_FILER_CASE_DAA_V1), vec![palw_replay_demand_queue_key_v1(h(2))]);
        assert_eq!(filer.expire_v1(12 + PALW_REPLAY_FILER_CASE_DAA_V1), vec![palw_replay_demand_queue_key_v1(h(3))]);
        // A demand is asked once a re-plan, at most twice, and not while queued.
        let floor_binding = {
            let (backend, class_id, artifact_root) = floor();
            misaka_palw_base0::produce::base0_material_decode_v1(&claim(&backend, class_id, artifact_root, false).1).unwrap().0
        };
        let binding = Box::new(floor_binding.clone());
        filer.note_v1(&duty(4, 7), PalwReplayMismatchSiteV1::Replay, 20, &own);
        filer.on_finding_v1(
            h(4),
            Ok(PalwReplayFindingV1::DemandLeaf { leaf: 3, binding: binding.clone(), why: "test".into(), rungs: 5 }),
            &own,
            &mut court_pending,
        );
        let moved = HashMap::new();
        assert_eq!(filer.demands_due_v1(30, &court_pending, &moved), vec![h(4)]);
        let key = palw_replay_demand_queue_key_v1(h(4));
        let queued = vec![(
            key.0,
            key.1,
            key.2,
            PalwConsensusObjectV2::DefaultAccusedHeld {
                accusation: Box::new(kaspa_consensus_core::palw_held_da_v1::PalwHeldAccusationV1 {
                    version: 1,
                    claim: h(4),
                    missing: kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: 3 },
                    accuser: own,
                    binding: *binding,
                    signature: vec![1],
                }),
            },
        )];
        assert!(filer.demands_due_v1(30, &queued, &moved).is_empty(), "not while queued");
        let mut moved = HashMap::new();
        moved.insert(key, 30);
        assert!(filer.demands_due_v1(35, &court_pending, &moved).is_empty(), "not a re-plan after its send");
        if let PalwReplayCaseStepV1::Demand { sends, .. } = &mut filer.cases.get_mut(&h(4)).unwrap().step {
            *sends = PALW_REPLAY_FILER_SENDS_V1;
        }
        assert!(filer.demands_due_v1(100, &court_pending, &moved).is_empty(), "two sends at most");
        // P2-8e's hook settles a fused leaf; nothing is queued.
        filer.note_v1(&duty(5, 7), PalwReplayMismatchSiteV1::Replay, 20, &own);
        let before = court_pending.len();
        filer.on_finding_v1(
            h(5),
            Ok(PalwReplayFindingV1::NeedsDissection { leaf: 9, binding: Box::new(floor_binding), rungs: 4 }),
            &own,
            &mut court_pending,
        );
        assert_eq!((filer.case(&h(5)).unwrap().step.clone(), court_pending.len()), (PalwReplayCaseStepV1::Settled, before));
        // The fence-off twin: dormant below `palw_rcore_plus`, and on a node that carries nothing.
        assert!(palw_replay_filer_armed_v1(true, true));
        assert!(!palw_replay_filer_armed_v1(false, true), "below palw_rcore_plus nothing runs");
        assert!(!palw_replay_filer_armed_v1(true, false), "a node that carries nothing keeps no book");
    }

    /// **One tick starts at most one run, and a case at a time runs** — the per-tick budget.
    #[tokio::test]
    async fn p2_8b_one_run_in_flight() {
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        filer.note_v1(&duty(2, 7), PalwReplayMismatchSiteV1::Replay, 10, &own);
        filer.note_v1(&duty(3, 7), PalwReplayMismatchSiteV1::Replay, 10, &own);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let handle = tokio::task::spawn_blocking(move || {
            let _ = rx.recv();
            Ok(PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LogitsAgree, rungs: 0 })
        });
        filer.started(h(2), handle);
        assert_eq!(filer.case(&h(2)).unwrap().runs, 1);
        assert_eq!(filer.due_v1(10), None, "one run in flight");
        assert_eq!(filer.expire_v1(10_000), vec![palw_replay_demand_queue_key_v1(h(3))], "the run in flight is never forgotten");
        tx.send(()).unwrap();
        let (claim, handle) = filer.running.take().unwrap();
        let mut court_pending = Vec::new();
        filer.on_finding_v1(claim, handle.await.unwrap(), &own, &mut court_pending);
        assert!(court_pending.is_empty());
    }
}
