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
//! * **P2-8b**: the claim's served captures (the copy this seat retained once it verified against
//!   the claim's roots, then the pool's) are sorted by what their bindings say
//!   ([`palw_replay_candidates_v1`]: gossiped bytes that are not the claim's are skipped, never a
//!   reason to stop), and tried in turn against this seat's own dense replay of the claim's job
//!   ([`palw_replay_contradiction_v1`], the ONE builder the real-claim suite also runs): the first
//!   divergent step becomes `StepArithmetic` → `ExecutorRefuted` (kind 4), or — step trees that agree
//!   — `LogitsNotStepOutput` (12, F1c). Filed through [`replay_file_seam_v1`].
//! * **P2-8d**: a divergent leaf located on the claim's own capture that the served material cannot
//!   open is DEMANDED: a `DefaultAccusedHeld` naming `StepLeaf { leaf }` (DA-3: free past
//!   `palw_rcore_plus`, from a seat or not, keyed by the session), asked of the chain through the
//!   fold's own gates for THAT unit (`palw_da_step_leaf_demand_check_v1` — never P2-6's row-0 read,
//!   whose once-per-accuser rule would drop the J-6 garbage path's second session) and queued on the
//!   court queue under its own key. The producer discloses the leaf's evidence (which the fold
//!   adjudicates: a guilty one convicts, `CourtFraud`) or defaults (S1).
//! * **P2-8e** (held dissections of fused-attention leaves) is ONE hook,
//!   [`palw_replay_held_dissection_hook_v1`]: it waits for the audit's A-held line.
//!
//! **Resources.** The loop does nothing per case but cheap reads: whether anything is served at all
//! (the pool's entry, the retained file's existence), the class, the claim's job. Everything that
//! decodes a capture runs in a blocking task ([`palw_replay_filer_run_v1`]): the retained file's read,
//! the candidates' sort, the dense capture's need (`whole_capture_memory_need_v1`, the capture arm's
//! figure: a class whose capture cannot be laid out whole is the streamed routes' and is not run
//! here) and its ledger reservation, held for the run's life, before the replay; every bisection rung
//! reserves its own two prefix reads before it runs ([`palw_replay_rung_bytes_v1`]). One run is in
//! flight, at most [`PALW_REPLAY_FILER_STARTS_PER_TICK_V1`] starts a tick, at most
//! [`PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1`] runs a claim, at most [`PALW_REPLAY_FILER_LOCAL_RUNS_V1`]
//! local replays a run, and a bisection spends at most `1 + ⌈log₂ n⌉` rungs
//! (`palw_replay_bisect_rungs_v1`). A run the ledger refuses, or that found nothing served yet that
//! is the claim's, gives its run back and waits — as SEAT-R's replays do; a rung the ledger refuses
//! mid-run is this host's failure and spends the run, so the claim's second run redoes it.
//!
//! **Filing is deduplicated.** One case a claim (the first trigger wins; a settled claim is
//! remembered for a case's window, so a standing trigger never reopens it), one kind-4 offence a
//! claim (its ledger key is the court queue's key, and the seam refuses it while it is queued or a
//! carrier of it is in flight), one demand a claim; a filing whose carrier was lost — refused by the
//! mempool or the builder, or sent and not landed while the duty stands — is queued once more, never
//! a third time; nothing is filed against this node's own bond.
//!
//! **Liveness.** An honest producer is never accused: kind 4 is filed only when the fold's own
//! predicate convicts the proof (the builder's rule), and a demand only for a leaf located on the
//! claim's own verified capture that it could not open — never after a proof was built and held.
//! A seat whose backend or class differs finds no verified capture, or a local run of another job,
//! and abstains.
//!
//! **Retentions a family does not lay out cost nothing** (the review's finding 5). A served capture
//! whose family's `capture_shape` refuses it — base0's floor reads the dense tuple only, so a folded
//! (v2) retention served for a floor claim — is sorted out BEFORE the run is priced or anything is
//! replayed (the node used to reserve, replay the whole job and only then abstain), and never ends the
//! search: the claim's own capture is in the form its family produces and reads (the floor's
//! free-prompt run retains the dense tuple; the model tiers fold — held attempts and free prompts
//! alike — and their `capture_shape` / `bisect_prefix_state` read the fold, `decode_any`). The
//! builder names such bytes `PalwReplayNothingV1::ServedNotBisectable`.
//!
//! **Consensus-inert, and dormant below `palw_rcore_plus`** ([`palw_replay_filer_armed_v1`]): nothing
//! is noted, run or filed there, and a node that carries nothing (no `--palw-fee-outpoint`) keeps no
//! book.

use super::*;
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwOffenceTargetV1};
use kaspa_consensus_core::palw_producer_v2::{PalwDaStepLeafDemandCheckV1, PalwSeatDutyV2};
use kaspa_consensus_core::palw_replay_refute_v1::{
    PalwReplayClaimV1, PalwReplayFindingV1, PalwReplayNothingV1, PalwReplayServedStandingV1, palw_replay_bisect_rungs_v1,
    palw_replay_contradiction_v1, palw_replay_executor_refuted_object_v1, palw_replay_served_standing_v1,
};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;

/// Runs a tick starts: one. A run is a whole dense replay of a claim's job, so the filer never
/// competes with the seat's own SEAT-R replays for more than one of the ledger's grants at a time.
pub(super) const PALW_REPLAY_FILER_STARTS_PER_TICK_V1: usize = 1;
/// Cases a tick tries to start before it gives the slot up — each one that waits is tried again
/// later, so these are distinct cases.
pub(super) const PALW_REPLAY_FILER_TRIES_PER_TICK_V1: usize = 4;
/// Runs one claim gets: the run, and one more after a failure that was this host's (a task that
/// did not finish, a local replay that did not run, a rung the ledger refused). A run's finding
/// stands: a replay that found nothing will find nothing again.
pub(super) const PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1: u8 = 2;
/// Claims the book pursues at once. A trigger past it is not noted (it is noted on a later tick,
/// if its duty still stands, once a case leaves) — never by evicting a case: an evicted settled
/// case came straight back on its standing trigger with a fresh run budget (the review's finding 7).
pub(super) const PALW_REPLAY_FILER_CASES_MAX_V1: usize = 128;
/// Settled claims remembered at once (their tombstones, one `Hash64` and a DAA each), each for
/// [`PALW_REPLAY_FILER_CASE_DAA_V1`] from its settling; past it the oldest is forgotten first.
pub(super) const PALW_REPLAY_FILER_TOMBSTONES_MAX_V1: usize = 4_096;
/// How long a noted case is pursued, from its note: `window_court` (3,000 DAA on testnet-12), the
/// longest a kind 4 or a demand can still act on the claim's stage (DA-8's post-Final window). A
/// settled claim is remembered as long, from its settling.
pub(super) const PALW_REPLAY_FILER_CASE_DAA_V1: u64 = 3_000;
/// Carriers one filing may take: the first, and one more when the first was lost while the claim's
/// duty still stands (a conviction ends the duty; a claim licensed by others is the court's).
pub(super) const PALW_REPLAY_FILER_SENDS_V1: u8 = 2;
/// When a sent filing counts as lost: six re-plans with the duty still standing — and, until then,
/// in flight (the seam refuses a second copy of it).
pub(super) const PALW_REPLAY_FILER_REFILE_DAA_V1: u64 = 6 * COURT_MOVE_REPLAN_DAA;
/// Served captures one run considers: this seat's retained copy and the pool's
/// (`MATERIALS_PER_CLAIM`) — every one the node holds for the claim.
pub(super) const PALW_REPLAY_FILER_CANDIDATES_MAX_V1: usize = 1 + MATERIALS_PER_CLAIM;
/// Local replays one run pays for: the attempt lane's candidates all answer the anchor's one job;
/// a free-prompt candidate names its own, so a run replays at most this many distinct jobs.
pub(super) const PALW_REPLAY_FILER_LOCAL_RUNS_V1: usize = 2;
/// How long a case whose served bytes held nothing of the claim's waits before it is sorted again
/// (a re-plan): the claim's own capture may arrive, and junk that is already held says nothing new.
pub(super) const PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1: u64 = COURT_MOVE_REPLAN_DAA;
/// The court queue's round of a kind-4 filing — keyed by its offence key
/// ([`palw_replay_refuted_queue_key_v1`]), a round no court move, DA accusation (`u32::MAX`), P2-8
/// reporter-filer object (`u32::MAX - 3 ..= u32::MAX - 1`, whose tick drops an `ObjectiveOffence`
/// on its reveal round that its book does not hold), P2-8c filing (`u32::MAX - 4`) or answer (a
/// folded unit digest) is keyed by.
pub(super) const PALW_REPLAY_REFUTED_QUEUE_ROUND_V1: u32 = u32::MAX - 5;
/// The court queue's round of a P2-8d `StepLeaf` demand ([`palw_replay_demand_queue_key_v1`]) — its
/// own, like every other lane's.
pub(super) const PALW_REPLAY_DEMAND_QUEUE_ROUND_V1: u32 = u32::MAX - 6;

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

/// Whether the court queue holds `key`.
fn queued_v1(court_pending: &[(Hash64, u32, bool, PalwConsensusObjectV2)], key: (Hash64, u32, bool)) -> bool {
    court_pending.iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key)
}

/// **The one seam P2-8b's `ExecutorRefuted` leaves through** — onto the panel's court queue, drained
/// by `carry_priority_v1` on the priority lane of P2-6's one scheduler (`PalwCarrierSlotsV1`), under
/// the offence's own key; `false` (nothing queued) when that key is queued already, or a carrier of
/// it went out less than [`PALW_REPLAY_FILER_REFILE_DAA_V1`] ago and may still land (`court_moved`,
/// the priority lane's own record of a send — whichever filer sent it: the review's finding 8).
///
/// P2-8 seam: routed through the reporter filer (PalwConvictionFilingV1) at integration.
///
/// Until then the object rides bare, reporter slot empty — a conviction without a commitment pays no
/// reporter (R-3), which P2-8's filer adds by committing over `(offence_key, evidence_id, reporter)`,
/// waiting for acceptance, then filing and revealing. What it needs is on the filing: the offence
/// key, the evidence id and the accused. At integration, a fault finder's trigger whose fault the
/// capture arm already refuted (P2-8's filer holds that refutation) is P2-8's to file, not a reason
/// for this filer to replay the claim.
pub(super) fn replay_file_seam_v1(
    court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    court_moved: &HashMap<(Hash64, u32, bool), u64>,
    current_daa: u64,
    filing: &PalwReplayFilingV1,
) -> bool {
    let key = palw_replay_refuted_queue_key_v1(filing.offence_key);
    let in_flight = court_moved.get(&key).is_some_and(|sent| current_daa < sent.saturating_add(PALW_REPLAY_FILER_REFILE_DAA_V1));
    if queued_v1(court_pending, key) || in_flight {
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
/// free-prompt lane — `roots.anchor` names it on both).
pub(super) struct PalwReplayServedV1 {
    pub capture: Vec<u8>,
    pub roots: PalwClaimRootsV1,
    /// The job's ids on the free-prompt lane.
    pub prompt_token_ids: Option<Vec<u32>>,
    pub work: ReplayWork,
}

/// **The lane a claim's served bytes are read on** — what the loop derives from the chain (the
/// attempt lane's job is the anchor's; a free-prompt payload names its own, held to the claim's class
/// and executor) and the blocking task reads the bytes against.
pub(super) enum PalwReplayLaneV1 {
    Attempt {
        job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
        prompt: Vec<usize>,
        roots: PalwClaimRootsV1,
    },
    /// `roots.anchor` is replaced by each payload's own job id (`fp_job_id_v3`).
    FreePrompt {
        class_id: Hash64,
        executor: PalwBondKeyV2,
        roots: PalwClaimRootsV1,
    },
}

/// **The served captures sorted by what their bindings say** ([`palw_replay_candidates_v1`]).
pub(super) struct PalwReplayCandidatesV1 {
    /// The claim's binding in a retention the family bisects, in the order held (retained first).
    pub usable: Vec<PalwReplayServedV1>,
    /// The claim's binding in a retention the family does not lay out (a fold served for a floor claim).
    pub not_bisectable: usize,
    /// Bytes that are not the claim's: another binding, no binding, not this lane's envelope.
    pub not_the_claims: usize,
}

/// **Sort a claim's served bytes before anything is paid for them** (the review's finding 1). The
/// pool takes any gossiped bytes (`pool_admit_material_v1`), so the producer of a garbage claim can
/// serve junk, or a decoy carrying the claim's binding, before (or instead of) its capture. Each
/// payload is read for its binding only — [`palw_replay_served_standing_v1`], the builder's own
/// reading — and one that is not the claim's is SKIPPED: it is never the payload the run is priced
/// or judged on, and it never stops the search. Exact duplicates (the retained copy is usually also
/// in the pool) are one candidate; at most [`PALW_REPLAY_FILER_CANDIDATES_MAX_V1`] are read.
pub(super) fn palw_replay_candidates_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    payloads: Vec<Vec<u8>>,
    lane: &PalwReplayLaneV1,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    execution_root: &Hash64,
) -> PalwReplayCandidatesV1 {
    let mut distinct: Vec<Vec<u8>> = Vec::new();
    for bytes in payloads {
        if distinct.len() >= PALW_REPLAY_FILER_CANDIDATES_MAX_V1 {
            break;
        }
        if !distinct.contains(&bytes) {
            distinct.push(bytes);
        }
    }
    let mut sorted = PalwReplayCandidatesV1 { usable: Vec::new(), not_bisectable: 0, not_the_claims: 0 };
    for bytes in distinct {
        let served = match lane {
            PalwReplayLaneV1::Attempt { job, prompt, roots } => Some(PalwReplayServedV1 {
                capture: bytes,
                roots: *roots,
                prompt_token_ids: None,
                work: ReplayWork::Attempt(job.clone(), prompt.clone()),
            }),
            PalwReplayLaneV1::FreePrompt { class_id, executor, roots } => {
                kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(&bytes, form)
                    .filter(|payload| payload.material.job.class_id == *class_id && payload.material.job.executor_bond == executor.0)
                    .map(|payload| {
                        let job = payload.material.job;
                        let prompt: Vec<usize> = payload.material.prompt_token_ids.iter().map(|t| *t as usize).collect();
                        PalwReplayServedV1 {
                            capture: payload.capture,
                            roots: PalwClaimRootsV1 { anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job), ..*roots },
                            prompt_token_ids: Some(payload.material.prompt_token_ids),
                            work: ReplayWork::FreePrompt(job, prompt),
                        }
                    })
            }
        };
        let Some(served) = served else {
            sorted.not_the_claims += 1;
            continue;
        };
        match palw_replay_served_standing_v1(backend, &served.capture, execution_root) {
            PalwReplayServedStandingV1::Bisectable => sorted.usable.push(served),
            PalwReplayServedStandingV1::NotBisectable => sorted.not_bisectable += 1,
            PalwReplayServedStandingV1::NotTheClaims => sorted.not_the_claims += 1,
        }
    }
    sorted
}

/// **One run's candidates, judged** — this seat's own dense replay of each candidate's job (at most
/// [`PALW_REPLAY_FILER_LOCAL_RUNS_V1`] distinct jobs, one local capture held at a time) and the ONE
/// builder, each bisection rung reserved on `ledger` (`role: "replay-bisect"`) for its two reads.
///
/// **Every candidate gets its turn** (the review's finding 1): captures that verify against the
/// claim's roots first (stable, so the retained copy leads among equals), then those that carry only
/// the claim's binding (F1c's: the garbage-logits capture no seat rule verifies). A finding on a
/// VERIFIED capture is the claim's and ends the run; `Nothing` on an unverified one — a decoy with
/// honest rows under the claim's binding — says nothing of the claim, and the next is tried. A
/// candidate that is not the claim's at all is skipped before its local replay is paid for.
///
/// `Err` is this host's failure: the local replay did not run, or the builder stopped on this
/// host's condition (`PalwReplayNothingV1::is_this_hosts`: a rung the ledger refused, the local
/// prefix unreadable — the review's finding 3), so the case keeps its second run. Every finding
/// about the claim is `Ok`.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_replay_filer_job_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    candidates: Vec<PalwReplayServedV1>,
    target: &PalwOffenceTargetV1,
    ladder: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
) -> Result<PalwReplayFindingV1, String> {
    let mut ordered: Vec<(bool, PalwReplayServedV1)> = candidates
        .into_iter()
        .take(PALW_REPLAY_FILER_CANDIDATES_MAX_V1)
        .map(|served| (backend.verify_material(&served.capture, served.roots) == PalwMaterialVerdictV1::Matches, served))
        .collect();
    ordered.sort_by_key(|(verified, _)| !*verified);
    let key = crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
        role: "replay-bisect",
        class_id: target.class_id,
        job: target.claim_id,
    };
    let mut local: Option<(Hash64, Vec<u8>)> = None;
    let mut local_runs = 0usize;
    let mut last = PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotTheClaims, rungs: 0 };
    for (verified, PalwReplayServedV1 { capture, roots, prompt_token_ids, work }) in ordered {
        if !verified
            && palw_replay_served_standing_v1(backend, &capture, &target.execution_root) != PalwReplayServedStandingV1::Bisectable
        {
            continue;
        }
        if local.as_ref().is_none_or(|(job, _)| *job != roots.anchor) {
            if local_runs >= PALW_REPLAY_FILER_LOCAL_RUNS_V1 {
                continue;
            }
            // One local capture at a time: the previous is dropped before the next replay runs, so
            // the run stays inside the one reservation its task holds.
            drop(local.take());
            local_runs += 1;
            let material = work.run(backend).ok_or("this seat's own replay of the claim's job did not run")?.material;
            local = Some((roots.anchor, material));
        }
        let (_, local_capture) = local.as_ref().expect("replayed above");
        let n = backend.capture_shape(&capture).map(|shape| shape.step_leaf_count).unwrap_or(0);
        let rung_bytes = palw_replay_rung_bytes_v1(capture.len() as u64, n);
        let finding = palw_replay_contradiction_v1(
            backend,
            &capture,
            local_capture,
            PalwReplayClaimV1 { target, roots, ladder, form, prompt_token_ids: prompt_token_ids.as_deref() },
            palw_replay_bisect_rungs_v1(n),
            |_rung| ledger.reserve(key.clone(), rung_bytes).map_err(|refusal| refusal.to_string()),
        );
        match &finding {
            PalwReplayFindingV1::Nothing { why, rungs } if why.is_this_hosts() => {
                return Err(format!("the bisection stopped on this host's condition after {rungs} rungs: {why:?}"));
            }
            PalwReplayFindingV1::Nothing { .. } if !verified => last = finding,
            _ => return Ok(finding),
        }
    }
    Ok(last)
}

/// **What the loop hands a run** — nothing borrowed from it: the served bytes (the retained file is
/// read in the task, first), the lane, and the claim the builder holds them to.
pub(super) struct PalwReplayRunInputV1 {
    pub retained: Option<std::path::PathBuf>,
    pub payloads: Vec<Vec<u8>>,
    pub lane: PalwReplayLaneV1,
    pub target: PalwOffenceTargetV1,
    pub ladder: u64,
    pub form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
}

/// **What one run came to**, as the book reads it ([`PalwReplayFilerV1::on_run_v1`]).
#[derive(Debug)]
pub(super) enum PalwReplayRunV1 {
    /// The builder ran and concluded.
    Found(PalwReplayFindingV1),
    /// Nothing ran that costs the claim a run: nothing served is the claim's yet, or the ledger
    /// refused the run. The run is given back and the case waits `retry_after` DAA.
    Wait { why: String, retry_after: u64 },
    /// Never on this seat: the claim's own capture (its authenticated binding) is past what this
    /// seat lays out whole — the streamed routes' (`whole_capture_memory_need_v1` refuses it).
    Never(String),
    /// This host's failure after the run was paid for; the claim's next run may redo it.
    HostFailed(String),
}

/// A memory need a run is priced at — the node's `PalwRoleMemoryNeedV1`, or a test's bytes.
pub(super) trait PalwReplayNeedV1 {
    fn bytes(&self) -> u64;
}

impl PalwReplayNeedV1 for crate::palw_backends::PalwRoleMemoryNeedV1 {
    fn bytes(&self) -> u64 {
        self.total_bytes()
    }
}

impl PalwReplayNeedV1 for u64 {
    fn bytes(&self) -> u64 {
        *self
    }
}

/// **One run, off the loop, start to end** — the blocking task's body.
///
/// 1. The served bytes: the retained copy (read here, never on the loop), then the pool's.
/// 2. Sorted ([`palw_replay_candidates_v1`]): none of the claim's in a form its family lays out →
///    `Wait` a re-plan, nothing priced or replayed (junk and unreadable forms say nothing, and the
///    claim's capture may still arrive — never `Never`: the review's findings 1 and 5).
/// 3. Priced at the largest need among the claim's candidates (`need_of`: the dense capture's
///    need, read off the candidate's own binding — which the sort made the claim's, so a refusal
///    here is the claim's shape and `Never`), and reserved (`reserve`, held to the run's end;
///    refused → `Wait` one DAA, the run given back).
/// 4. [`palw_replay_filer_job_v1`].
pub(super) fn palw_replay_filer_run_v1<N: PalwReplayNeedV1, G>(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    input: PalwReplayRunInputV1,
    mut need_of: impl FnMut(&[u8]) -> Result<N, String>,
    reserve: impl FnOnce(&N) -> Result<G, String>,
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
) -> PalwReplayRunV1 {
    let PalwReplayRunInputV1 { retained, payloads, lane, target, ladder, form } = input;
    let held: Vec<Vec<u8>> = retained.and_then(|path| std::fs::read(path).ok()).into_iter().chain(payloads).collect();
    let sorted = palw_replay_candidates_v1(backend, held, &lane, form, &target.execution_root);
    if sorted.usable.is_empty() {
        return PalwReplayRunV1::Wait {
            why: format!(
                "no served capture carries the claim's binding in a retention this family bisects ({} not the claim's, {} not \
                 bisectable)",
                sorted.not_the_claims, sorted.not_bisectable
            ),
            retry_after: PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1,
        };
    }
    let mut need: Option<N> = None;
    let mut refused = None;
    for served in &sorted.usable {
        match need_of(&served.capture) {
            Ok(each) => {
                if need.as_ref().is_none_or(|most| each.bytes() > most.bytes()) {
                    need = Some(each);
                }
            }
            Err(why) => refused = refused.or(Some(why)),
        }
    }
    let Some(need) = need else {
        return PalwReplayRunV1::Never(format!(
            "the claim's capture is not laid out whole on this seat ({})",
            refused.unwrap_or_default()
        ));
    };
    let _held_for_the_run = match reserve(&need) {
        Ok(guard) => guard,
        Err(why) => return PalwReplayRunV1::Wait { why, retry_after: 1 },
    };
    match palw_replay_filer_job_v1(backend, sorted.usable, &target, ladder, form, ledger) {
        Ok(finding) => PalwReplayRunV1::Found(finding),
        Err(why) => PalwReplayRunV1::HostFailed(why),
    }
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

/// **Is anything served at all?** The loop's only per-case read of the served bytes (the review's
/// finding 9): the pool's count and whether the retained file exists — no read, no decode, no job
/// derivation while nothing is held.
pub(super) fn palw_replay_served_held_v1(pool_payloads: usize, retained_exists: bool) -> Result<(), PalwReplayStartV1> {
    if pool_payloads == 0 && !retained_exists {
        // Nothing served that could be the claim's: the SEAT-R replay refuted a claim whose
        // producer served this seat nothing, which is P2-6's accusation, not a proof.
        return Err(PalwReplayStartV1::Wait("no served capture of the claim is held".into()));
    }
    Ok(())
}

/// **P2-8d: the chain's answer to a `StepLeaf` demand as a step** (the review's finding 2). The fold's
/// own gates for the named unit ([`PalwDaStepLeafDemandCheckV1`]) — so a seat that accused row 0
/// before files its demand once that session has closed, and a non-seat files too (DA-3). Waited out:
/// this accuser's open session (its row-0 accusation) and A-6's room. Settled: the leaf answered on
/// chain (the fold adjudicated it) and every other refusal of the fold's gate, which stands for the
/// case's window (as P2-6 reads it: stage, retention, standing, DA-8's budgets).
pub(super) fn palw_replay_demand_step_v1(check: &PalwDaStepLeafDemandCheckV1) -> PalwSeatAccuseStepV1 {
    match check {
        PalwDaStepLeafDemandCheckV1::File { .. } => PalwSeatAccuseStepV1::File,
        PalwDaStepLeafDemandCheckV1::SessionOpen
        | PalwDaStepLeafDemandCheckV1::Refused(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::AccusationExposureCeiling {
            ..
        }) => PalwSeatAccuseStepV1::Retry,
        PalwDaStepLeafDemandCheckV1::Answered | PalwDaStepLeafDemandCheckV1::Refused(_) => PalwSeatAccuseStepV1::Settle,
    }
}

/// Where one case stands. A case that settles leaves the book for its tombstone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwReplayCaseStepV1 {
    /// Noted; the builder has not run, or gave its run back, or ran into this host's failure and
    /// may run again.
    Waiting,
    /// The one run in flight.
    Running,
    /// Kind 4 handed to the seam, `sends` times, last at `queued_at`.
    Filed { filing: Box<PalwReplayFilingV1>, sends: u8, queued_at: u64 },
    /// P2-8d: the `StepLeaf` demand, asked of the chain until it lands or settles. `sends` carriers
    /// queued; `refused_at` the DAA the chain last asked it to wait.
    Demand { leaf: u64, binding: Box<PalwStepBindingV2>, sends: u8, refused_at: Option<u64> },
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

/// **The filer's book** — the panel loop holds one. Cases by claim, the claims settled (tombstones),
/// the court-queue keys settled cases were debounced under, and the one run in flight.
#[derive(Default)]
pub(super) struct PalwReplayFilerV1 {
    cases: BTreeMap<Hash64, PalwReplayCaseV1>,
    /// claim → the DAA it settled (or expired) at: not noted again for a case's window.
    settled: BTreeMap<Hash64, u64>,
    /// Court-queue keys of cases that left the book since the last drain, for `court_moved` to drop.
    released: Vec<(Hash64, u32, bool)>,
    running: Option<(Hash64, tokio::task::JoinHandle<PalwReplayRunV1>)>,
}

/// What a finished run does to its case ([`PalwReplayFilerV1::on_run_v1`]).
enum PalwReplayNextV1 {
    Step(PalwReplayCaseStepV1),
    Wait { refund: bool, retry_at: u64 },
    Settle,
}

impl PalwReplayFilerV1 {
    /// **Note a mismatch on `duty`'s claim.** `false` when nothing was noted: the claim is this
    /// node's own (never filed against its own bond), a case for it exists (the first trigger wins),
    /// it settled within a case's window (its tombstone: a standing trigger does not reopen it), or
    /// the book is full (the trigger is noted on a later tick; nothing is evicted).
    pub(super) fn note_v1(
        &mut self,
        duty: &PalwSeatDutyV2,
        site: PalwReplayMismatchSiteV1,
        current_daa: u64,
        own_bond: &PalwBondKeyV2,
    ) -> bool {
        if duty.executor_bond == *own_bond
            || self.cases.contains_key(&duty.claim_id)
            || self.settled.contains_key(&duty.claim_id)
            || self.cases.len() >= PALW_REPLAY_FILER_CASES_MAX_V1
        {
            return false;
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

    /// The court-queue keys a case may have been debounced under: its demand's, and its filing's.
    fn keys_of(claim: &Hash64, case: &PalwReplayCaseV1) -> Vec<(Hash64, u32, bool)> {
        let mut keys = vec![palw_replay_demand_queue_key_v1(*claim)];
        if let PalwReplayCaseStepV1::Filed { filing, .. } = &case.step {
            keys.push(palw_replay_refuted_queue_key_v1(filing.offence_key));
        }
        keys
    }

    /// **The case leaves the book for its tombstone** — remembered from `current_daa` for a case's
    /// window (bounded: the oldest tombstone goes first), its queue keys released.
    fn settle_at(&mut self, claim: &Hash64, current_daa: u64) {
        let Some(case) = self.cases.remove(claim) else { return };
        self.released.extend(Self::keys_of(claim, &case));
        self.settled.insert(*claim, current_daa);
        while self.settled.len() > PALW_REPLAY_FILER_TOMBSTONES_MAX_V1 {
            let Some(oldest) = self.settled.iter().min_by_key(|(claim, at)| (**at, **claim)).map(|(claim, _)| *claim) else { break };
            self.settled.remove(&oldest);
        }
    }

    /// Whether `claim` settled within a case's window.
    #[cfg(test)]
    pub(super) fn settled_v1(&self, claim: &Hash64) -> bool {
        self.settled.contains_key(claim)
    }

    /// **Forget what a case's window has passed** — cases noted more than
    /// [`PALW_REPLAY_FILER_CASE_DAA_V1`] ago (never the one in flight, whose task still holds its
    /// reservation until it returns), which leave for their tombstones, and tombstones older than it.
    pub(super) fn expire_v1(&mut self, current_daa: u64) {
        let running = self.running.as_ref().map(|(claim, _)| *claim);
        let old: Vec<Hash64> = self
            .cases
            .iter()
            .filter(|(claim, case)| {
                Some(**claim) != running && current_daa > case.noted_daa.saturating_add(PALW_REPLAY_FILER_CASE_DAA_V1)
            })
            .map(|(claim, _)| *claim)
            .collect();
        for claim in &old {
            self.settle_at(claim, current_daa);
        }
        self.settled.retain(|_, at| current_daa <= at.saturating_add(PALW_REPLAY_FILER_CASE_DAA_V1));
    }

    /// **The court-queue keys cases that left the book were debounced under** — for `court_moved`
    /// to drop (nothing else prunes it). Drained.
    pub(super) fn take_released_v1(&mut self) -> Vec<(Hash64, u32, bool)> {
        std::mem::take(&mut self.released)
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

    /// A start on `claim` had to wait: it is not tried again before `retry_at`, so the cases behind
    /// it are.
    fn wait(&mut self, claim: &Hash64, retry_at: u64) {
        if let Some(case) = self.cases.get_mut(claim) {
            case.retry_at = retry_at;
        }
    }

    pub(super) fn case(&self, claim: &Hash64) -> Option<&PalwReplayCaseV1> {
        self.cases.get(claim)
    }

    /// A run started on `claim`: its run is spent (given back if the run only waited).
    fn started(&mut self, claim: Hash64, handle: tokio::task::JoinHandle<PalwReplayRunV1>) {
        if let Some(case) = self.cases.get_mut(&claim) {
            case.runs += 1;
            case.step = PalwReplayCaseStepV1::Running;
        }
        self.running = Some((claim, handle));
    }

    /// **What a finished run makes of its case** — the pure half of [`PalwPanelService::replay_filer_tick_v1`]'s
    /// poll. `Refutes` becomes a filing through the seam (kind 4 built by
    /// `palw_replay_executor_refuted_object_v1`, the gate's own encoding); `DemandLeaf` a P2-8d demand;
    /// `NeedsDissection` P2-8e's hook; `Nothing` about the claim settles; a run that only waited gives
    /// its run back; a host failure waits for the claim's next run (or settles when none is left).
    /// Returns the filing queued now, if any.
    pub(super) fn on_run_v1(
        &mut self,
        claim: Hash64,
        run: PalwReplayRunV1,
        current_daa: u64,
        own_bond: &PalwBondKeyV2,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) -> Option<PalwReplayFilingV1> {
        let case = self.cases.get_mut(&claim)?;
        let host_failed = |case: &PalwReplayCaseV1| {
            if case.runs < PALW_REPLAY_FILER_RUNS_PER_CLAIM_V1 {
                PalwReplayNextV1::Wait { refund: false, retry_at: current_daa.saturating_add(1) }
            } else {
                PalwReplayNextV1::Settle
            }
        };
        let mut filed = None;
        let next = match run {
            PalwReplayRunV1::Found(PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs }) => {
                let executor = case.duty.executor_bond;
                if executor == *own_bond {
                    PalwReplayNextV1::Settle
                } else {
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
                                "[{PALW_PANEL}] claim {claim}: this seat's replay refutes its executor at {site:?} ({rungs} bisection \
                                 rungs, found by its {:?} check) — filing ExecutorRefuted (offence {}, evidence {}; ADR-0152 J-4, \
                                 SR-8, P2-8b)",
                                case.site, filing.offence_key, filing.evidence_id
                            );
                            let queued = replay_file_seam_v1(court_pending, court_moved, current_daa, &filing);
                            let step = PalwReplayCaseStepV1::Filed {
                                filing: Box::new(filing.clone()),
                                sends: u8::from(queued),
                                queued_at: current_daa,
                            };
                            filed = queued.then_some(filing);
                            PalwReplayNextV1::Step(step)
                        }
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {claim}: the refutation at {site:?} cannot be filed: {why}");
                            PalwReplayNextV1::Settle
                        }
                    }
                }
            }
            PalwReplayRunV1::Found(PalwReplayFindingV1::DemandLeaf { leaf, binding, why, rungs }) => {
                info!(
                    "[{PALW_PANEL}] claim {claim}: the first divergent step is leaf {leaf} ({rungs} bisection rungs) and {why} — \
                     demanding it in a data-availability session (ADR-0152 DA-3, J-6, P2-8d)"
                );
                PalwReplayNextV1::Step(PalwReplayCaseStepV1::Demand { leaf, binding, sends: 0, refused_at: None })
            }
            PalwReplayRunV1::Found(PalwReplayFindingV1::NeedsDissection { leaf, binding, .. }) => {
                palw_replay_held_dissection_hook_v1(&claim, leaf, &binding);
                PalwReplayNextV1::Settle
            }
            // A host's condition that reached the book as a finding (the job maps it to `Err`; kept
            // here so no path settles a claim on it).
            PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing { why, rungs }) if why.is_this_hosts() => {
                warn!("[{PALW_PANEL}] claim {claim}: the replay filer's run stopped on this host ({why:?}, {rungs} rungs)");
                host_failed(case)
            }
            PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing { why, rungs }) => {
                info!("[{PALW_PANEL}] claim {claim}: the replay filer files nothing ({why:?}, {rungs} bisection rungs)");
                PalwReplayNextV1::Settle
            }
            PalwReplayRunV1::Wait { why, retry_after } => {
                crate::palw_backends::note_throttled_v1("panel-replay-filer-wait", || {
                    format!("[{PALW_PANEL}] claim {claim}: the replay filer's run waits — {why}")
                });
                PalwReplayNextV1::Wait { refund: true, retry_at: current_daa.saturating_add(retry_after.max(1)) }
            }
            PalwReplayRunV1::Never(why) => {
                info!("[{PALW_PANEL}] claim {claim}: the replay filer does not run — {why}");
                PalwReplayNextV1::Settle
            }
            PalwReplayRunV1::HostFailed(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the replay filer's run failed on this host: {why}");
                host_failed(case)
            }
        };
        match next {
            PalwReplayNextV1::Step(step) => case.step = step,
            PalwReplayNextV1::Wait { refund, retry_at } => {
                if refund {
                    case.runs = case.runs.saturating_sub(1);
                }
                case.step = PalwReplayCaseStepV1::Waiting;
                case.retry_at = retry_at;
            }
            PalwReplayNextV1::Settle => self.settle_at(&claim, current_daa),
        }
        filed
    }

    /// **Filings whose carrier was lost** (the review's finding 4), queued once more through the seam
    /// while the claim's duty stands and a send is left — never while the filing is still queued:
    ///
    /// * **sent and not landed**: a carrier went out (`court_moved` holds its send) at least
    ///   [`PALW_REPLAY_FILER_REFILE_DAA_V1`] ago;
    /// * **never sent**: it left the queue without a send on record, a re-plan or more after it was
    ///   queued — the priority lane drops an object the mempool or the carrier builder refused, and
    ///   writes `court_moved` only for a submit that succeeded.
    pub(super) fn refile_lost_v1(
        &mut self,
        current_daa: u64,
        live_duty: impl Fn(&Hash64) -> bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) -> usize {
        let mut refiled = 0;
        for (claim, case) in self.cases.iter_mut() {
            let PalwReplayCaseStepV1::Filed { filing, sends, queued_at } = &mut case.step else { continue };
            let key = palw_replay_refuted_queue_key_v1(filing.offence_key);
            if *sends >= PALW_REPLAY_FILER_SENDS_V1 || queued_v1(court_pending, key) || !live_duty(claim) {
                continue;
            }
            let lost = match court_moved.get(&key) {
                Some(sent) => current_daa >= sent.saturating_add(PALW_REPLAY_FILER_REFILE_DAA_V1),
                None => current_daa >= queued_at.saturating_add(COURT_MOVE_REPLAN_DAA),
            };
            if lost && replay_file_seam_v1(court_pending, court_moved, current_daa, filing) {
                *sends += 1;
                *queued_at = current_daa;
                refiled += 1;
            }
        }
        refiled
    }

    /// The P2-8d demands the chain is asked about now, with their leaves: not queued, not sent or
    /// asked to wait less than a re-plan ago, a send left.
    pub(super) fn demands_due_v1(
        &self,
        current_daa: u64,
        court_pending: &[(Hash64, u32, bool, PalwConsensusObjectV2)],
        court_moved: &HashMap<(Hash64, u32, bool), u64>,
    ) -> Vec<(Hash64, u64)> {
        let fresh = |at: Option<u64>| at.is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA));
        self.cases
            .iter()
            .filter_map(|(claim, case)| {
                let PalwReplayCaseStepV1::Demand { leaf, sends, refused_at, .. } = &case.step else { return None };
                let key = palw_replay_demand_queue_key_v1(*claim);
                (*sends < PALW_REPLAY_FILER_SENDS_V1
                    && !queued_v1(court_pending, key)
                    && !fresh(court_moved.get(&key).copied())
                    && !fresh(*refused_at))
                .then_some((*claim, *leaf))
            })
            .collect()
    }

    /// **The demands queued and waiting for the slot, asked again** (as P2-6 re-asks its queued
    /// accusations): one the chain now asks to wait leaves the queue unsent (its send given back)
    /// and is asked again a re-plan later; one it refuses for good leaves with its case. `ask` is the
    /// chain's answer as a step ([`palw_replay_demand_step_v1`] of the tip's read); `None` (no tip)
    /// keeps it queued.
    pub(super) fn recheck_queued_demands_v1(
        &mut self,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        mut ask: impl FnMut(Hash64, u64) -> Option<PalwSeatAccuseStepV1>,
    ) {
        let mut settle = Vec::new();
        court_pending.retain(|(claim, round, responder, _)| {
            if (*claim, *round, *responder) != palw_replay_demand_queue_key_v1(*claim) {
                return true;
            }
            let Some(PalwReplayCaseV1 { step: PalwReplayCaseStepV1::Demand { leaf, sends, refused_at, .. }, .. }) =
                self.cases.get_mut(claim)
            else {
                return false;
            };
            match ask(*claim, *leaf) {
                None | Some(PalwSeatAccuseStepV1::File) => true,
                Some(PalwSeatAccuseStepV1::Retry) => {
                    *sends = sends.saturating_sub(1);
                    *refused_at = Some(current_daa);
                    false
                }
                Some(PalwSeatAccuseStepV1::Settle) => {
                    settle.push(*claim);
                    false
                }
            }
        });
        for claim in settle {
            self.settle_at(&claim, current_daa);
        }
    }
}

impl PalwPanelService {
    /// **P2-8b / P2-8d, once a tick** — the filer's one call site in the panel loop.
    ///
    /// 1. Notes this tick's triggers on live duties: a SEAT-R replay that refuted (`replay_refuted`)
    ///    and a fault a fault finder recorded (`seat_found_fault_v1`).
    /// 2. Polls the run in flight and files what it found ([`PalwReplayFilerV1::on_run_v1`]);
    ///    queues once more a filing whose carrier was lost.
    /// 3. Asks the chain about each P2-8d demand — queued ones again, due ones before they are built
    ///    (`palw_da_step_leaf_demand_check_v1`, the fold's own gates for the named leaf) — and queues
    ///    the ones it would open.
    /// 4. Starts at most [`PALW_REPLAY_FILER_STARTS_PER_TICK_V1`] run, off the loop.
    /// 5. Drops the `court_moved` debounce of every case that left the book.
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
        filer.expire_v1(current_daa);
        // 2. The run in flight.
        if filer.running.as_ref().is_some_and(|(_, handle)| handle.is_finished()) {
            let (claim, handle) = filer.running.take().expect("checked above");
            let run =
                handle.await.unwrap_or_else(|e| PalwReplayRunV1::HostFailed(format!("the replay filer's task did not finish: {e}")));
            filer.on_run_v1(claim, run, current_daa, &bond_key, court_pending, court_moved);
        }
        let live: HashSet<Hash64> = duties.iter().map(|duty| duty.claim_id).collect();
        let refiled = filer.refile_lost_v1(current_daa, |claim| live.contains(claim), court_pending, court_moved);
        if refiled > 0 {
            info!(
                "[{PALW_PANEL}] queued {refiled} ExecutorRefuted filing(s) again: the carrier was lost while the duty stands (P2-8b)"
            );
        }
        // 3. P2-8d's demands: the queued ones asked again, then the due ones.
        filer.recheck_queued_demands_v1(current_daa, court_pending, |claim, leaf| {
            session.palw_da_step_leaf_demand_check_v1(claim, bond_key, leaf).map(|check| palw_replay_demand_step_v1(&check))
        });
        for (claim, leaf) in filer.demands_due_v1(current_daa, court_pending, court_moved) {
            self.replay_demand_v1(session, filer, claim, leaf, current_daa, network_domain, bond_key, court_pending);
        }
        // 4. The next run: at most one start, over at most a few tries, so a case that waits (no
        // served capture yet, the claim's block not held) never holds the ones behind it.
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
                // Nothing served, the claim's block not held: tried again at the next DAA, the run
                // not spent.
                Err(PalwReplayStartV1::Wait(why)) => {
                    crate::palw_backends::note_throttled_v1("panel-replay-filer-wait", || {
                        format!("[{PALW_PANEL}] claim {claim}: the replay filer's run waits — {why}")
                    });
                    filer.wait(&claim, current_daa.saturating_add(1));
                }
                Err(PalwReplayStartV1::Never(why)) => {
                    info!("[{PALW_PANEL}] claim {claim}: the replay filer does not run — {why}");
                    filer.settle_at(&claim, current_daa);
                }
            }
        }
        // 5. The debounce leaves with the case: nothing else prunes `court_moved`.
        for key in filer.take_released_v1() {
            court_moved.remove(&key);
        }
    }

    /// **One P2-8d demand, asked of the chain and queued** — the fold's own gates for the named leaf
    /// (`palw_da_step_leaf_demand_check_v1`: this accuser's open session, the leaf answered, C-8 with
    /// A-6's room — the review's finding 2) as a step ([`palw_replay_demand_step_v1`]), then the ONE
    /// held builder (`palw_da_held_accusation_object_v1`, which asks the fold's stateless halves of
    /// the unit first).
    #[allow(clippy::too_many_arguments)]
    fn replay_demand_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReplayFilerV1,
        claim: Hash64,
        leaf: u64,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    ) {
        let Some(check) = session.palw_da_step_leaf_demand_check_v1(claim, bond_key, leaf) else { return };
        match palw_replay_demand_step_v1(&check) {
            PalwSeatAccuseStepV1::File => {}
            PalwSeatAccuseStepV1::Retry => {
                if let Some(PalwReplayCaseV1 { step: PalwReplayCaseStepV1::Demand { refused_at, .. }, .. }) =
                    filer.cases.get_mut(&claim)
                {
                    *refused_at = Some(current_daa);
                }
                return;
            }
            PalwSeatAccuseStepV1::Settle => {
                info!("[{PALW_PANEL}] claim {claim}: the StepLeaf demand settles — {check:?} (P2-8d)");
                filer.settle_at(&claim, current_daa);
                return;
            }
        }
        let Some(case) = filer.cases.get_mut(&claim) else { return };
        let PalwReplayCaseStepV1::Demand { binding, sends, .. } = &mut case.step else { return };
        let (execution_root, class_id) = (case.duty.execution_root, case.duty.class_id);
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
                filer.settle_at(&claim, current_daa);
            }
        }
    }

    /// **Start one run** — on the loop, only what the chain answers and a cheap look at what is
    /// served (the review's finding 9): nothing held at all waits BEFORE the class is resolved or the
    /// job derived; the attempt lane's anchor and job come off the claim's block (no block, no
    /// anchor: wait — never a default anchor, which `verify_material` reads as "skip the anchor
    /// check"). The rest — the retained file, the payloads' decoding, the need and its reservation,
    /// the replay — runs in the blocking task ([`palw_replay_filer_run_v1`]).
    fn replay_filer_start_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &PalwSeatDutyV2,
        network_domain: Hash64,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
    ) -> Result<tokio::task::JoinHandle<PalwReplayRunV1>, PalwReplayStartV1> {
        let retained = self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id));
        let pool = materials.get(&duty.claim_id).map(Vec::as_slice).unwrap_or_default();
        let retained_exists = retained.is_file();
        palw_replay_served_held_v1(pool.len(), retained_exists)?;
        let backend = self
            .resolve_backend(session, duty.class_id, duty.artifact_root)
            .map_err(|e| PalwReplayStartV1::Never(format!("the class: {e}")))?;
        let form = self.class_prompt_ids_form(duty.class_id);
        let ladder = self.class_step_ladder(duty.class_id);
        let roots = PalwClaimRootsV1 {
            execution_root: duty.execution_root,
            trace_root: duty.trace_root,
            anchor: Hash64::default(),
            attempt_draw: None,
            output_root: Some(duty.output_root),
            job_pin: duty.fp_job_pin_v1(),
        };
        let lane = if duty.free_prompt {
            PalwReplayLaneV1::FreePrompt { class_id: duty.class_id, executor: duty.executor_bond, roots }
        } else {
            let no_block = || PalwReplayStartV1::Wait("the claim's block is not in this node's store".into());
            let anchor = self
                .job_anchor_for_claim(
                    session,
                    backend.as_ref(),
                    network_domain,
                    duty.accepted_block,
                    duty.class_id,
                    &duty.executor_bond,
                )
                .ok_or_else(no_block)?;
            let (job, prompt) = self
                .attempt_job_for_claim(
                    session,
                    backend.as_ref(),
                    network_domain,
                    duty.accepted_block,
                    duty.class_id,
                    &duty.executor_bond,
                )
                .ok_or_else(no_block)?;
            let attempt_draw = self.attempt_draw_for_claim(session, duty.accepted_block);
            PalwReplayLaneV1::Attempt { job, prompt, roots: PalwClaimRootsV1 { anchor, attempt_draw, ..roots } }
        };
        let registry = self.backends();
        let carriage = self.chain_carriage_v1(session, duty.class_id);
        let input = PalwReplayRunInputV1 {
            retained: retained_exists.then_some(retained),
            payloads: pool.iter().take(PALW_REPLAY_FILER_CANDIDATES_MAX_V1).cloned().collect(),
            lane,
            target: palw_replay_target_of_duty_v1(duty),
            ladder,
            form,
        };
        let (class_id, artifact_root, claim_id) = (duty.class_id, duty.artifact_root, duty.claim_id);
        // A start whose served bytes turn out to hold nothing of the claim's waits in its task, so
        // this is traced rather than said: the run's outcome is what the operator reads.
        trace!("[{PALW_PANEL}] claim {claim_id}: a replay filer run starts off the loop (P2-8b)");
        Ok(tokio::task::spawn_blocking(move || {
            palw_replay_filer_run_v1(
                backend.as_ref(),
                input,
                |capture| {
                    registry
                        .whole_capture_memory_need_v1(backend.as_ref(), class_id, artifact_root, capture, ladder, |_| carriage.clone())
                },
                |need| reserve_replay_on_host_v1("replay-filer", need, class_id, claim_id),
                &crate::palw_memory_ledger::host_ledger_v1(),
            )
        }))
    }
}

/// Why a run did not start this tick.
#[derive(Debug)]
pub(super) enum PalwReplayStartV1 {
    /// Asked again next tick, the run not spent: nothing served yet, the claim's block not held.
    Wait(String),
    /// Never on this seat: the class does not resolve.
    Never(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_offence_attribution_v1::PALW_FALSE_VALID_NETWORK_LADDER_V1;
    use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
    use kaspa_consensus_core::palw_replay_refute_v1::PalwReplaySiteV1;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }

    const MERKLE: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 =
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;

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
                .with_prompt_ids_form(MERKLE);
        (backend, entry.class_id(), root)
    }

    /// A real floor claim: its job, the capture it serves, its roots, the target a node builds from
    /// its duty, and the faulted leaf.
    struct Claim {
        job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
        prompt: Vec<usize>,
        capture: Vec<u8>,
        roots: PalwClaimRootsV1,
        target: PalwOffenceTargetV1,
        leaf: Option<u64>,
    }

    impl Claim {
        fn lane(&self) -> PalwReplayLaneV1 {
            PalwReplayLaneV1::Attempt { job: self.job.clone(), prompt: self.prompt.clone(), roots: self.roots }
        }

        fn served(&self, capture: Vec<u8>) -> PalwReplayServedV1 {
            PalwReplayServedV1 {
                capture,
                roots: self.roots,
                prompt_token_ids: None,
                work: ReplayWork::Attempt(self.job.clone(), self.prompt.clone()),
            }
        }

        fn run_input(&self, payloads: Vec<Vec<u8>>) -> PalwReplayRunInputV1 {
            PalwReplayRunInputV1 {
                retained: None,
                payloads,
                lane: self.lane(),
                target: self.target.clone(),
                ladder: PALW_FALSE_VALID_NETWORK_LADDER_V1,
                form: MERKLE,
            }
        }
    }

    /// The claim a producer commits to `run`'s roots under `binding_root`, as a node's duty names it.
    fn claim_of(
        class_id: Hash64,
        artifact_root: Hash64,
        job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
        prompt: Vec<usize>,
        capture: Vec<u8>,
        (execution_root, trace_root, output_root): (Hash64, Hash64, Hash64),
        leaf: Option<u64>,
    ) -> Claim {
        let anchor = h(0x5EED_0001);
        let roots = PalwClaimRootsV1 {
            execution_root,
            trace_root,
            anchor,
            attempt_draw: Some(true),
            output_root: Some(output_root),
            job_pin: None,
        };
        let mut d = duty(0x42, 7);
        (d.class_id, d.artifact_root, d.execution_root, d.trace_root, d.output_root) =
            (class_id, artifact_root, execution_root, trace_root, output_root);
        Claim { job, prompt, capture, roots, target: palw_replay_target_of_duty_v1(&d), leaf }
    }

    /// A real floor attempt job, the honest run, and — when `lie` — the drill's one-lane lie at the
    /// first openable leaf from the middle of the step space on, re-committed (a self-consistent
    /// garbage trace only a replay finds).
    fn claim(backend: &misaka_palw_base0::backend::Base0Backend, class_id: Hash64, artifact_root: Hash64, lie: bool) -> Claim {
        let (canonical, prompt) = backend.job_for_anchor(h(0x5EED_0001)).expect("the floor derives a job");
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
        let roots = (run.execution_root, run.trace_root, run.output_root);
        claim_of(class_id, artifact_root, job, prompt, run.material, roots, leaf)
    }

    /// **The F1c residual on a real floor run** (the T46 harness's `Fault::GarbageLogits`, rebuilt
    /// here from public parts): the honest step tree, the selecting row's runner-up lane bent toward —
    /// never past — the argmax, the logits root and the execution root re-committed over the bent
    /// rows. Returns the claim (whose served capture is the bent tuple, which no seat rule verifies)
    /// and a DECOY: the honest rows under the same bent binding — the claim's binding, honest logits.
    fn bent_claim(backend: &misaka_palw_base0::backend::Base0Backend, class_id: Hash64, artifact_root: Hash64) -> (Claim, Vec<u8>) {
        use kaspa_consensus_core::palw_step_leg::{checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1};
        let honest = claim(backend, class_id, artifact_root, false);
        let (mut binding, tiles, honest_rows, ids, chunks) =
            misaka_palw_base0::produce::base0_material_decode_v1(&honest.capture).expect("the capture decodes");
        let mut rows = honest_rows.clone();
        let row = &mut rows[0];
        let top = (0..row.len()).max_by_key(|i| (row[*i], std::cmp::Reverse(*i))).unwrap();
        let lane = (0..row.len()).find(|i| *i != top && row[*i] < row[top] - 1).expect("a lane below the argmax by two");
        row[lane] += 1;
        binding.full_logits_trace_root =
            kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(&binding.job_context, &rows, &ids);
        let ctx_hash = binding.job_context.context_hash();
        let step_root =
            step_leg_root_v1(&ctx_hash, &binding.shape_profile.shape_profile_id(), binding.step_leaf_count, &binding.step_merkle_root);
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &binding.checkpoint_profile.profile_hash(),
            &binding.state_chunk_map_id,
            binding.job_context.exact_decode_tokens.saturating_sub(1),
            binding.checkpoint_count,
            &binding.checkpoint_merkle_root,
        );
        binding.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).expect("the bent commitment is well-formed");
        let bent = borsh::to_vec(&(&binding, &tiles, &rows, &ids, &chunks)).expect("encodes");
        let decoy = borsh::to_vec(&(&binding, &tiles, &honest_rows, &ids, &chunks)).expect("encodes");
        let roots = (binding.committed_execution_root, binding.full_logits_trace_root, honest.roots.output_root.unwrap());
        (claim_of(class_id, artifact_root, honest.job, honest.prompt, bent, roots, None), decoy)
    }

    /// The captured tuple with one held step tile's preimage changed: the claim's own binding, a body
    /// that is not its execution — no seat rule verifies it.
    fn tampered(capture: &[u8]) -> Vec<u8> {
        let (binding, mut tiles, rows, ids, chunks) = misaka_palw_base0::produce::base0_material_decode_v1(capture).expect("decodes");
        tiles.remove(0);
        borsh::to_vec(&(&binding, &tiles, &rows, &ids, &chunks)).expect("encodes")
    }

    fn ledger() -> Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1> {
        crate::palw_memory_ledger::PalwMemoryLedgerV1::new(crate::palw_memory_ledger::PalwMemoryPoolV1::Host, Some(1 << 40), || None)
    }

    /// A run's need as the node prices it, minus the registry: the capture decodes (either retention).
    fn need_of(capture: &[u8]) -> Result<u64, String> {
        misaka_palw_base0::produce::base0_material_decode_any_v1(capture)
            .map(|_| 1)
            .map_err(|_| "the capture does not decode".to_string())
    }

    /// **T54f (node half), P2-8b on a real floor run**: the seat's own replay bisects the served lying
    /// capture to the injected leaf, in at most `1 + ⌈log₂ n⌉` rungs each reserved on the ledger
    /// (the O(log n) bound on a real trace) and released with its rung, and the proof it builds is
    /// kind 4's `StepArithmetic` that the fold's own predicate convicts; the object encodes it as the
    /// gate decodes it, under the per-claim ledger key, and the seam queues it once under that key —
    /// and not again while a carrier of it may still land (finding 8).
    #[test]
    fn p2_8b_a_real_lie_is_bisected_to_its_leaf_in_log_rungs_and_filed_once() {
        let (backend, class_id, artifact_root) = floor();
        let c = claim(&backend, class_id, artifact_root, true);
        let leaf = c.leaf.unwrap();
        let n = backend.capture_shape(&c.capture).unwrap().step_leaf_count;
        let ledger = ledger();
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![c.served(c.capture.clone())],
            &c.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
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
            c.target.execution_root,
            c.target.artifact_root,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
        )
        .expect("the fold's own predicate convicts it");
        // The node's book: noted once, filed once through the seam, under the offence's own key.
        let d = PalwSeatDutyV2 { claim_id: c.target.claim_id, executor_bond: c.target.executor_bond, ..duty(0x42, 7) };
        let mut filer = PalwReplayFilerV1::default();
        assert!(filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 1_000, &bond(1)));
        assert!(!filer.note_v1(&d, PalwReplayMismatchSiteV1::FaultFinder, 1_001, &bond(1)), "one case a claim");
        let mut court_pending = Vec::new();
        let mut moved = HashMap::new();
        let found = PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs };
        let filing =
            filer.on_run_v1(d.claim_id, PalwReplayRunV1::Found(found), 1_000, &bond(1), &mut court_pending, &moved).expect("filed");
        let offence =
            kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&d.executor_bond.0, &d.claim_id);
        assert_eq!((filing.offence_key, filing.accused), (offence, d.executor_bond));
        assert_eq!(court_pending.len(), 1);
        let key = palw_replay_refuted_queue_key_v1(offence);
        assert_eq!((court_pending[0].0, court_pending[0].1, court_pending[0].2), key);
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
        assert!(!replay_file_seam_v1(&mut court_pending, &moved, 1_000, &filing), "the seam queues one filing an offence");
        // Finding 8: sent (by this filer or P2-8's, under the same key) — no second copy while it may land.
        court_pending.clear();
        moved.insert(key, 2_000);
        assert!(!replay_file_seam_v1(&mut court_pending, &moved, 2_000 + PALW_REPLAY_FILER_REFILE_DAA_V1 - 1, &filing), "in flight");
        assert!(court_pending.is_empty());
        // The lost carrier is queued once more while the duty stands, never a third time.
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

    /// **Finding 4: a filing the mempool or the carrier builder refused is re-filed.** The priority
    /// lane drops such an object without a `court_moved` entry; once a re-plan has passed with the
    /// key neither queued nor sent, it is queued once more (sends 1 → 2), and never a third time.
    #[test]
    fn p2_8b_a_filing_the_mempool_refused_is_queued_once_more() {
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        let d = duty(0x42, 7);
        assert!(filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 100, &own));
        let offence = h(0x0FFE);
        let filing = PalwReplayFilingV1 {
            claim_id: d.claim_id,
            offence_key: offence,
            evidence_id: h(0xE1D),
            accused: d.executor_bond,
            object: PalwConsensusObjectV2::ObjectiveOffence {
                kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
                accused: d.executor_bond,
                evidence_id: h(0xE1D),
                evidence: vec![1],
            },
        };
        filer.cases.get_mut(&d.claim_id).unwrap().step =
            PalwReplayCaseStepV1::Filed { filing: Box::new(filing), sends: 1, queued_at: 100 };
        let moved = HashMap::new();
        let mut court_pending = Vec::new();
        // The carrier lane took it off the queue and the mempool refused it: nothing queued, nothing sent.
        assert_eq!(filer.refile_lost_v1(100 + COURT_MOVE_REPLAN_DAA - 1, |_| true, &mut court_pending, &moved), 0, "a re-plan first");
        assert_eq!(filer.refile_lost_v1(100 + COURT_MOVE_REPLAN_DAA, |_| true, &mut court_pending, &moved), 1, "lost: queued again");
        assert_eq!(court_pending.len(), 1);
        assert_eq!(filer.refile_lost_v1(500, |_| true, &mut court_pending, &moved), 0, "not while it is queued");
        court_pending.clear();
        assert_eq!(filer.refile_lost_v1(500, |_| true, &mut court_pending, &moved), 0, "two sends at most");
        let PalwReplayCaseStepV1::Filed { sends, queued_at, .. } = &filer.case(&d.claim_id).unwrap().step else { panic!() };
        assert_eq!((*sends, *queued_at), (2, 100 + COURT_MOVE_REPLAN_DAA));
    }

    /// **An honest claim produces no filing**: its capture reproduces the roots and so does this
    /// seat's replay — `LocalReproduces`, no rung spent, nothing queued; and a capture that is not the
    /// claim's (another run's roots) is not the committed execution — `ServedNotTheClaims`.
    #[test]
    fn p2_8b_an_honest_claim_files_nothing() {
        let (backend, class_id, artifact_root) = floor();
        let c = claim(&backend, class_id, artifact_root, false);
        let ledger = ledger();
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![c.served(c.capture.clone())],
            &c.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &ledger,
        )
        .unwrap();
        assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, rungs: 0 });
        let other =
            PalwReplayServedV1 { roots: PalwClaimRootsV1 { execution_root: h(0xBAD), ..c.roots }, ..c.served(c.capture.clone()) };
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![other],
            &PalwOffenceTargetV1 { execution_root: h(0xBAD), ..c.target },
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &ledger,
        )
        .unwrap();
        assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotTheClaims, rungs: 0 });
        let mut filer = PalwReplayFilerV1::default();
        let d = duty(0x42, 7);
        assert!(filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 1_000, &bond(1)));
        let mut court_pending = Vec::new();
        assert!(
            filer
                .on_run_v1(d.claim_id, PalwReplayRunV1::Found(finding), 1_000, &bond(1), &mut court_pending, &HashMap::new())
                .is_none()
        );
        assert!(court_pending.is_empty() && filer.settled_v1(&d.claim_id) && filer.case(&d.claim_id).is_none());
        assert_eq!(filer.due_v1(1_000), None, "a settled case is not run again");
    }

    /// **Finding 1, junk first: gossiped bytes never make the filer give up.** The pool holds junk
    /// ahead of the claim's lying capture: the sort skips it (the node used to price the run off the
    /// first payload and answer `Never` when it did not decode), prices the run off the claim's own,
    /// and the run refutes at the lie's leaf. With nothing of the claim's served yet the run gives its
    /// run back and waits — never `Never`.
    #[test]
    fn p2_8b_junk_served_first_never_stops_the_filer() {
        let (backend, class_id, artifact_root) = floor();
        let c = claim(&backend, class_id, artifact_root, true);
        let junk = vec![0x5Au8; 70];
        assert!(need_of(&junk).is_err(), "the junk does not decode: the node's old start answered Never on it");
        let sorted = palw_replay_candidates_v1(
            &backend,
            vec![junk.clone(), c.capture.clone(), c.capture.clone()],
            &c.lane(),
            MERKLE,
            &c.target.execution_root,
        );
        assert_eq!((sorted.usable.len(), sorted.not_the_claims, sorted.not_bisectable), (1, 1, 0), "junk skipped, the duplicate one");
        assert_eq!(sorted.usable[0].capture, c.capture);
        let mut priced = Vec::new();
        let run = palw_replay_filer_run_v1(
            &backend,
            c.run_input(vec![junk.clone(), c.capture.clone()]),
            |capture| {
                priced.push(capture.to_vec());
                need_of(capture)
            },
            |_| Ok::<(), String>(()),
            &ledger(),
        );
        let PalwReplayRunV1::Found(PalwReplayFindingV1::Refutes { site, .. }) = run else { panic!("refuted past the junk: {run:?}") };
        assert_eq!(site, PalwReplaySiteV1::StepLeaf(c.leaf.unwrap()));
        assert_eq!(priced, vec![c.capture.clone()], "priced off the claim's capture, never the junk");
        // Nothing of the claim's yet: wait a re-plan, the run given back.
        let run = palw_replay_filer_run_v1(&backend, c.run_input(vec![junk]), need_of, |_| Ok::<(), String>(()), &ledger());
        assert!(matches!(run, PalwReplayRunV1::Wait { retry_after: PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1, .. }), "{run:?}");
        // The ledger refuses the run: wait one DAA, nothing replayed.
        let run = palw_replay_filer_run_v1(
            &backend,
            c.run_input(vec![c.capture.clone()]),
            need_of,
            |_| Err::<(), _>("full".into()),
            &ledger(),
        );
        assert!(matches!(run, PalwReplayRunV1::Wait { retry_after: 1, .. }), "{run:?}");
        // The book gives a waiting run back.
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        let d = duty(0x42, 7);
        filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 10, &own);
        filer.cases.get_mut(&d.claim_id).unwrap().runs = 1;
        let wait = PalwReplayRunV1::Wait { why: "nothing yet".into(), retry_after: PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1 };
        filer.on_run_v1(d.claim_id, wait, 10, &own, &mut Vec::new(), &HashMap::new());
        let case = filer.case(&d.claim_id).unwrap();
        assert_eq!(
            (case.runs, &case.step, case.retry_at),
            (0, &PalwReplayCaseStepV1::Waiting, 10 + PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1)
        );
        // The loop's own look: nothing held at all waits before anything is derived.
        assert!(matches!(palw_replay_served_held_v1(0, false), Err(PalwReplayStartV1::Wait(_))));
        assert!(palw_replay_served_held_v1(1, false).is_ok() && palw_replay_served_held_v1(0, true).is_ok());
    }

    /// **Finding 1, a decoy first: every candidate gets its turn.** F1c's garbage-logits capture never
    /// verifies (SEAT-0's head rule is the lie), so the node reads captures that carry only the claim's
    /// binding. A decoy — the claim's own bent binding over HONEST rows — comes first: its logits agree
    /// with this seat's replay, `Nothing`, which on an unverified capture says nothing of the claim
    /// (the node used to try that one candidate and settle); the claim's bent capture is tried next and
    /// refuted by 12. A tampered copy of a verified lie ahead of the real one fares the same.
    #[test]
    fn p2_8b_a_decoy_carrying_the_binding_first_never_hides_the_claims_capture() {
        let (backend, class_id, artifact_root) = floor();
        let (bent, decoy) = bent_claim(&backend, class_id, artifact_root);
        assert_eq!(backend.verify_material(&bent.capture, bent.roots), PalwMaterialVerdictV1::Mismatch, "no seat verifies the lie");
        assert_eq!(backend.verify_material(&decoy, bent.roots), PalwMaterialVerdictV1::Mismatch);
        for capture in [&bent.capture, &decoy] {
            assert_eq!(
                palw_replay_served_standing_v1(&backend, capture, &bent.target.execution_root),
                PalwReplayServedStandingV1::Bisectable
            );
        }
        let ledger = ledger();
        let alone = palw_replay_filer_job_v1(
            &backend,
            vec![bent.served(decoy.clone())],
            &bent.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &ledger,
        )
        .unwrap();
        assert!(matches!(alone, PalwReplayFindingV1::Nothing { .. }), "the decoy alone proves nothing: {alone:?}");
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![bent.served(decoy.clone()), bent.served(bent.capture.clone())],
            &bent.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &ledger,
        )
        .expect("one local replay");
        let PalwReplayFindingV1::Refutes { contradiction, site, rungs, .. } = finding else {
            panic!("12 past the decoy: {finding:?}")
        };
        assert!(matches!(site, PalwReplaySiteV1::LogitsHead { row: 0, .. }) && rungs == 0, "{site:?}");
        assert!(matches!(contradiction, PalwPanelContradictionV1::LogitsNotStepOutput { .. }));
        // Through the whole run, the decoy first in the pool.
        let run = palw_replay_filer_run_v1(
            &backend,
            bent.run_input(vec![decoy, bent.capture.clone()]),
            need_of,
            |_| Ok::<(), String>(()),
            &ledger,
        );
        assert!(matches!(run, PalwReplayRunV1::Found(PalwReplayFindingV1::Refutes { .. })), "{run:?}");
        // A verified lie behind a tampered copy of itself: the verified one is judged, whatever the order.
        let (backend, class_id, artifact_root) = floor();
        let c = claim(&backend, class_id, artifact_root, true);
        let copy = tampered(&c.capture);
        assert_eq!(backend.verify_material(&copy, c.roots), PalwMaterialVerdictV1::Mismatch);
        let finding = palw_replay_filer_job_v1(
            &backend,
            vec![c.served(copy), c.served(c.capture.clone())],
            &c.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &ledger,
        )
        .unwrap();
        assert!(
            matches!(finding, PalwReplayFindingV1::Refutes { site: PalwReplaySiteV1::StepLeaf(l), .. } if Some(l) == c.leaf),
            "{finding:?}"
        );
    }

    /// **The floor, as a family that does not lay out one served retention** — every verb is the
    /// floor's, but `capture_shape` refuses the bytes `unlaid` (a family reading a form it does not
    /// produce: base0 reads the dense tuple only, and a fold of a floor run cannot even be made).
    struct Unlaid<'a>(&'a misaka_palw_base0::backend::Base0Backend, Vec<u8>);

    impl PalwExecutionBackendV1 for Unlaid<'_> {
        fn model_id(&self) -> &str {
            self.0.model_id()
        }
        fn job_for_anchor(&self, anchor: Hash64) -> Result<(kaspa_consensus_core::palw_v2::PalwJobContextV2, Vec<usize>), String> {
            self.0.job_for_anchor(anchor)
        }
        fn execute(
            &self,
            job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
            prompt: &[usize],
        ) -> Result<kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1, String> {
            self.0.execute(job, prompt)
        }
        fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
            self.0.verify_material(material, claim)
        }
        fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
            (material != self.1.as_slice()).then(|| self.0.capture_shape(material)).flatten()
        }
        fn disclose_trace_event(
            &self,
            material: &[u8],
            row: u32,
            tile: u8,
        ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
            self.0.disclose_trace_event(material, row, tile)
        }
        fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<Hash64> {
            self.0.bisect_prefix_state(material, index)
        }
    }

    /// **Finding 5: a retention the family does not lay out costs nothing and stops nothing.** The
    /// reviewer's premise does not hold for the families as built — the floor's own free-prompt run
    /// retains the dense tuple, which it bisects, and the model tiers' folds (held attempts and free
    /// prompts) are read by their own `capture_shape` / `bisect_prefix_state` (`decode_any`) — but
    /// the guard it asked for is kept: bytes carrying the claim's own binding in a form the family
    /// does not lay out are named by the builder (`ServedNotBisectable`), sorted `NotBisectable`, and
    /// a run holding only them waits WITHOUT pricing, reserving or replaying anything (the node used
    /// to reserve the dense need and replay the whole job before it abstained); served ahead of the
    /// claim's capture they are skipped, and the claim's capture is judged.
    #[test]
    fn p2_8b_a_retention_the_family_does_not_lay_out_is_never_replayed() {
        let floor_backend = super::super::seat_s_tests::floor_backend();
        let (job, ids) = super::super::seat_s_tests::floor_fp_job(&floor_backend);
        let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
        let fp = floor_backend.execute_free_prompt(&job, &prompt).expect("the producer's run");
        assert!(floor_backend.capture_shape(&fp.outcome.material).is_some(), "the floor's free-prompt retention is the dense tuple");
        let (floor, class_id, artifact_root) = floor();
        let c = claim(&floor, class_id, artifact_root, false);
        let unlaid = tampered(&c.capture);
        let backend = Unlaid(&floor, unlaid.clone());
        assert_eq!(
            palw_replay_served_standing_v1(&backend, &unlaid, &c.target.execution_root),
            PalwReplayServedStandingV1::NotBisectable,
            "the claim's own binding, in a form this family does not lay out"
        );
        let finding = palw_replay_contradiction_v1(
            &backend,
            &unlaid,
            &c.capture,
            PalwReplayClaimV1 {
                target: &c.target,
                roots: c.roots,
                ladder: PALW_FALSE_VALID_NETWORK_LADDER_V1,
                form: MERKLE,
                prompt_token_ids: None,
            },
            palw_replay_bisect_rungs_v1(1),
            |_| -> Result<(), String> { panic!("no rung") },
        );
        assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotBisectable, rungs: 0 });
        // The run holding only those bytes: nothing priced, reserved or replayed — it waits.
        let run = palw_replay_filer_run_v1(
            &backend,
            c.run_input(vec![unlaid.clone()]),
            |_| -> Result<u64, String> { panic!("never priced") },
            |_| -> Result<(), String> { panic!("never reserved") },
            &ledger(),
        );
        assert!(matches!(run, PalwReplayRunV1::Wait { retry_after: PALW_REPLAY_FILER_UNSERVED_RETRY_DAA_V1, .. }), "{run:?}");
        // Ahead of the claim's capture: skipped; the claim's is priced and judged.
        let mut priced = Vec::new();
        let run = palw_replay_filer_run_v1(
            &backend,
            c.run_input(vec![unlaid, c.capture.clone()]),
            |capture| {
                priced.push(capture.to_vec());
                need_of(capture)
            },
            |_| Ok::<(), String>(()),
            &ledger(),
        );
        assert!(
            matches!(run, PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, .. })),
            "{run:?}"
        );
        assert_eq!(priced, vec![c.capture.clone()]);
    }

    /// **Finding 3: a rung the ledger refuses is this host's failure, never a finding.** The builder
    /// stops at the refused rung (nothing past it runs, the reservation is the rung's); the job
    /// reports it as `Err`, and the book puts the case back to `Waiting` with its second run, due at
    /// the next DAA — the second refusal settles it (the run budget bounds a host that never has room).
    #[test]
    fn p2_8b_a_refused_rung_is_this_hosts_failure_and_the_claim_runs_again() {
        let (backend, class_id, artifact_root) = floor();
        let c = claim(&backend, class_id, artifact_root, true);
        let tiny =
            crate::palw_memory_ledger::PalwMemoryLedgerV1::new(crate::palw_memory_ledger::PalwMemoryPoolV1::Host, Some(1), || None);
        let outcome = palw_replay_filer_job_v1(
            &backend,
            vec![c.served(c.capture.clone())],
            &c.target,
            PALW_FALSE_VALID_NETWORK_LADDER_V1,
            MERKLE,
            &tiny,
        );
        let Err(why) = outcome else { panic!("a refused rung is this host's: {outcome:?}") };
        assert!(why.contains("RungRefused"), "{why}");
        assert_eq!(tiny.reserved_bytes(), 0);
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        let d = duty(0x42, 7);
        filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 10, &own);
        filer.cases.get_mut(&d.claim_id).unwrap().runs = 1;
        filer.on_run_v1(d.claim_id, PalwReplayRunV1::HostFailed(why.clone()), 10, &own, &mut Vec::new(), &HashMap::new());
        assert_eq!(filer.case(&d.claim_id).unwrap().step, PalwReplayCaseStepV1::Waiting, "the second run");
        assert_eq!(filer.due_v1(11), Some(d.claim_id), "due at the next DAA");
        assert!(!filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 11, &own), "still the one case");
        filer.cases.get_mut(&d.claim_id).unwrap().runs = 2;
        filer.on_run_v1(d.claim_id, PalwReplayRunV1::HostFailed(why), 12, &own, &mut Vec::new(), &HashMap::new());
        assert!(filer.settled_v1(&d.claim_id), "two runs a claim");
        // A host stop that reached the book as a finding is read the same way.
        let mut filer = PalwReplayFilerV1::default();
        filer.note_v1(&d, PalwReplayMismatchSiteV1::Replay, 10, &own);
        filer.cases.get_mut(&d.claim_id).unwrap().runs = 1;
        let stop = kaspa_consensus_core::palw_replay_refute_v1::PalwReplayBisectStopV1::RungRefused { rung: 3, why: "full".into() };
        let found = PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::Bisect(stop), rungs: 3 };
        filer.on_run_v1(d.claim_id, PalwReplayRunV1::Found(found), 10, &own, &mut Vec::new(), &HashMap::new());
        assert_eq!(filer.case(&d.claim_id).unwrap().step, PalwReplayCaseStepV1::Waiting);
    }

    /// **The book: never this node's own claim, one case a claim, one run at a time, a run a claim
    /// spent on a host failure at most twice, a full book that evicts nothing and a settled claim that
    /// stays settled (finding 7), and the fence-off twin.**
    #[test]
    fn p2_8b_the_book_is_bounded_and_deduplicated() {
        let own = bond(1);
        let mut filer = PalwReplayFilerV1::default();
        assert!(!filer.note_v1(&duty(1, 1), PalwReplayMismatchSiteV1::Replay, 10, &own), "never against this node's own bond");
        assert!(filer.note_v1(&duty(2, 7), PalwReplayMismatchSiteV1::Replay, 10, &own));
        assert!(filer.note_v1(&duty(3, 7), PalwReplayMismatchSiteV1::FaultFinder, 11, &own));
        assert_eq!(filer.due_v1(11), Some(h(2)), "the oldest first");
        // A start that waits is tried again later, and the case behind it is tried meanwhile.
        filer.wait(&h(2), 12);
        assert_eq!(filer.due_v1(11), Some(h(3)), "the next case, while the first waits");
        assert_eq!(filer.due_v1(12), Some(h(2)), "the first again at its DAA, its run unspent");
        assert_eq!(filer.case(&h(2)).unwrap().runs, 0);
        // A host failure keeps the case for its second run, then settles it.
        for (run, settled) in [(1u8, false), (2, true)] {
            filer.cases.get_mut(&h(2)).unwrap().runs = run;
            filer.on_run_v1(
                h(2),
                PalwReplayRunV1::HostFailed("the task did not finish".into()),
                12,
                &own,
                &mut Vec::new(),
                &HashMap::new(),
            );
            assert_eq!(filer.settled_v1(&h(2)), settled, "after run {run}");
        }
        assert_eq!(filer.due_v1(12), Some(h(3)));
        assert!(!filer.note_v1(&duty(2, 7), PalwReplayMismatchSiteV1::Replay, 13, &own), "a settled claim is not reopened");
        assert_eq!(filer.take_released_v1(), vec![palw_replay_demand_queue_key_v1(h(2))], "its debounce is released");
        // Expiry sends a case to its tombstone; the tombstone outlives it by a window.
        filer.expire_v1(12 + PALW_REPLAY_FILER_CASE_DAA_V1);
        assert!(filer.case(&h(3)).is_none() && filer.settled_v1(&h(3)));
        assert_eq!(filer.take_released_v1(), vec![palw_replay_demand_queue_key_v1(h(3))]);
        assert!(!filer.note_v1(&duty(3, 7), PalwReplayMismatchSiteV1::Replay, 13 + PALW_REPLAY_FILER_CASE_DAA_V1, &own));
        filer.expire_v1(13 + 2 * PALW_REPLAY_FILER_CASE_DAA_V1);
        assert!(!filer.settled_v1(&h(2)) && !filer.settled_v1(&h(3)), "tombstones expire");
        // Finding 7: a full book notes nothing more and evicts nothing; settled cases stay settled
        // however many triggers stand (the reviewer's probe re-opened 147 in 20 ticks).
        let mut filer = PalwReplayFilerV1::default();
        for n in 0..PALW_REPLAY_FILER_CASES_MAX_V1 as u64 {
            assert!(filer.note_v1(&duty(1_000 + n, 7), PalwReplayMismatchSiteV1::Replay, 20, &own));
        }
        assert!(!filer.note_v1(&duty(9_999, 7), PalwReplayMismatchSiteV1::Replay, 20, &own), "full: not noted");
        for n in 0..PALW_REPLAY_FILER_CASES_MAX_V1 as u64 {
            let nothing = PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, rungs: 0 };
            filer.on_run_v1(h(1_000 + n), PalwReplayRunV1::Found(nothing), 21, &own, &mut Vec::new(), &HashMap::new());
        }
        let mut reopened = 0;
        for tick in 0..20u64 {
            for n in 0..=PALW_REPLAY_FILER_CASES_MAX_V1 as u64 {
                reopened += usize::from(filer.note_v1(&duty(1_000 + n, 7), PalwReplayMismatchSiteV1::Replay, 22 + tick, &own));
            }
        }
        assert_eq!(reopened, 1, "only the trigger that was never noted is noted; no settled case reopens");
        // A demand is asked once a re-plan, at most twice, and not while queued.
        let floor_binding = {
            let (backend, class_id, artifact_root) = floor();
            misaka_palw_base0::produce::base0_material_decode_v1(&claim(&backend, class_id, artifact_root, false).capture).unwrap().0
        };
        let mut filer = PalwReplayFilerV1::default();
        let binding = Box::new(floor_binding.clone());
        filer.note_v1(&duty(4, 7), PalwReplayMismatchSiteV1::Replay, 20, &own);
        let demand = PalwReplayFindingV1::DemandLeaf { leaf: 3, binding: binding.clone(), why: "test".into(), rungs: 5 };
        filer.on_run_v1(h(4), PalwReplayRunV1::Found(demand), 20, &own, &mut Vec::new(), &HashMap::new());
        let moved = HashMap::new();
        assert_eq!(filer.demands_due_v1(30, &[], &moved), vec![(h(4), 3)]);
        let key = palw_replay_demand_queue_key_v1(h(4));
        let object = PalwConsensusObjectV2::DefaultAccusedHeld {
            accusation: Box::new(kaspa_consensus_core::palw_held_da_v1::PalwHeldAccusationV1 {
                version: 1,
                claim: h(4),
                missing: kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: 3 },
                accuser: own,
                binding: *binding,
                signature: vec![1],
            }),
        };
        let mut queued = vec![(key.0, key.1, key.2, object)];
        assert!(filer.demands_due_v1(30, &queued, &moved).is_empty(), "not while queued");
        let mut sent = HashMap::new();
        sent.insert(key, 30);
        assert!(filer.demands_due_v1(35, &[], &sent).is_empty(), "not a re-plan after its send");
        // A queued demand the chain now asks to wait leaves the queue unsent, its send given back.
        if let PalwReplayCaseStepV1::Demand { sends, .. } = &mut filer.cases.get_mut(&h(4)).unwrap().step {
            *sends = 1;
        }
        let mut asked = Vec::new();
        filer.recheck_queued_demands_v1(40, &mut queued, |claim, leaf| {
            asked.push((claim, leaf));
            Some(PalwSeatAccuseStepV1::Retry)
        });
        assert!(queued.is_empty() && asked == vec![(h(4), 3)]);
        let PalwReplayCaseStepV1::Demand { sends, refused_at, .. } = &filer.case(&h(4)).unwrap().step else { panic!() };
        assert_eq!((*sends, *refused_at), (0, Some(40)));
        if let PalwReplayCaseStepV1::Demand { sends, .. } = &mut filer.cases.get_mut(&h(4)).unwrap().step {
            *sends = PALW_REPLAY_FILER_SENDS_V1;
        }
        assert!(filer.demands_due_v1(100, &[], &sent).is_empty(), "two sends at most");
        // P2-8e's hook settles a fused leaf; nothing is queued.
        filer.note_v1(&duty(5, 7), PalwReplayMismatchSiteV1::Replay, 20, &own);
        let mut court_pending = Vec::new();
        let dissection = PalwReplayFindingV1::NeedsDissection { leaf: 9, binding: Box::new(floor_binding), rungs: 4 };
        filer.on_run_v1(h(5), PalwReplayRunV1::Found(dissection), 20, &own, &mut court_pending, &HashMap::new());
        assert!(filer.settled_v1(&h(5)) && court_pending.is_empty());
        // The fence-off twin: dormant below `palw_rcore_plus`, and on a node that carries nothing.
        assert!(palw_replay_filer_armed_v1(true, true));
        assert!(!palw_replay_filer_armed_v1(false, true), "below palw_rcore_plus nothing runs");
        assert!(!palw_replay_filer_armed_v1(true, false), "a node that carries nothing keeps no book");
    }

    /// **Finding 2: the demand is asked by the fold's gates for its OWN unit.** The chain's answer as a
    /// step: `File` for a seat AND for a non-seat (DA-3 — P2-6's rule settled the latter); this
    /// accuser's open session (its row-0 accusation) and A-6's room wait; the leaf answered and every
    /// other refusal settle. (P2-6's `AccusedBefore` / `Answered` are not this read's answers at all:
    /// the J-6 garbage path's second session is filed — T54f runs it on the real harness.)
    #[test]
    fn p2_8d_the_demand_step_reads_the_named_leafs_own_gate() {
        use kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1;
        use kaspa_consensus_core::palw_state_v2::{PalwDaAdmissionV1, PalwStateV2Error};
        let admission =
            |accuser_is_seat| PalwDaAdmissionV1 { stage: PalwDaStageV1::Live, accuser_is_seat, exposure: 1, deadline_daa: 9 };
        let step = |check: PalwDaStepLeafDemandCheckV1| palw_replay_demand_step_v1(&check);
        assert_eq!(step(PalwDaStepLeafDemandCheckV1::File { admission: admission(true) }), PalwSeatAccuseStepV1::File);
        assert_eq!(
            step(PalwDaStepLeafDemandCheckV1::File { admission: admission(false) }),
            PalwSeatAccuseStepV1::File,
            "a non-seat files"
        );
        assert_eq!(step(PalwDaStepLeafDemandCheckV1::SessionOpen), PalwSeatAccuseStepV1::Retry, "its row-0 session closes first");
        let ceiling = PalwStateV2Error::AccusationExposureCeiling {
            bond: bond(1),
            edge: "data-availability session",
            backed: 0,
            accusation: 1,
            ceiling: 1,
        };
        assert_eq!(step(PalwDaStepLeafDemandCheckV1::Refused(ceiling)), PalwSeatAccuseStepV1::Retry, "A-6's room");
        assert_eq!(step(PalwDaStepLeafDemandCheckV1::Answered), PalwSeatAccuseStepV1::Settle);
        assert_eq!(step(PalwDaStepLeafDemandCheckV1::Refused(PalwStateV2Error::DaCourtDormant)), PalwSeatAccuseStepV1::Settle);
    }

    /// **The tick wires the filer where it runs** (finding 6): after P2-6's accusations and before the
    /// collector and submitter (whose priority lane carries the court queue), once; the demand asks the
    /// named leaf's own gate, never P2-6's row-0 read; the start looks at what is served before it
    /// derives the job, and turns a missing anchor into a wait rather than a default; the run is priced
    /// and reserved inside its blocking task.
    #[test]
    fn the_tick_runs_the_filer_before_the_carriers_and_the_start_looks_before_it_derives() {
        let panel = include_str!("palw_panel.rs");
        let accusations = panel.find("// --- P2-6: this seat's accusations of withholding ---").expect("P2-6's step");
        let tick = panel.find("self.replay_filer_tick_v1(").expect("the filer's call");
        let carriers = panel.find("// --- the collector + submitter's half ---").expect("the carriers");
        assert!(accusations < tick && tick < carriers, "after P2-6, before the priority lane carries the court queue");
        assert_eq!(panel.matches("self.replay_filer_tick_v1(").count(), 1, "one call a tick");
        let whole = include_str!("palw_filer_replay.rs");
        let source = &whole[..whole.find("#[cfg(test)]\nmod tests").expect("the production half")];
        let demand = &source[source.find("fn replay_demand_v1(").expect("the demand")..];
        let demand = &demand[..demand.find("\n    }\n").expect("its body")];
        assert!(demand.contains("session.palw_da_step_leaf_demand_check_v1(claim, bond_key, leaf)"));
        assert!(demand.contains("palw_replay_demand_step_v1(&check)"));
        assert!(!source.contains("session.palw_da_accusation_check_v1("), "never P2-6's row-0 read");
        let start = &source[source.find("fn replay_filer_start_v1(").expect("the start")..];
        let start = &start[..start.find("\n    }\n").expect("its body")];
        let looked = start.find("palw_replay_served_held_v1(").expect("looks first");
        assert!(
            looked < start.find(".resolve_backend(").expect("the class")
                && looked < start.find(".job_anchor_for_claim(").expect("the job")
        );
        let anchor = &start[start.find(".job_anchor_for_claim(").unwrap()..start.find(".attempt_job_for_claim(").expect("the job")];
        assert!(anchor.contains(".ok_or_else(no_block)?;") && !anchor.contains("unwrap_or_default"), "no block, no anchor: wait");
        assert!(!start.contains("std::fs::read("), "the retained file is read in the task");
        let spawned = start.find("tokio::task::spawn_blocking(").expect("the task");
        assert!(start.find("whole_capture_memory_need_v1(").unwrap() > spawned, "priced in the task");
        assert!(start.find("reserve_replay_on_host_v1(").unwrap() > spawned, "reserved in the task");
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
            PalwReplayRunV1::Found(PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LogitsAgree, rungs: 0 })
        });
        filer.started(h(2), handle);
        assert_eq!(filer.case(&h(2)).unwrap().runs, 1);
        assert_eq!(filer.due_v1(10), None, "one run in flight");
        filer.expire_v1(10_000);
        assert!(filer.case(&h(2)).is_some() && filer.case(&h(3)).is_none(), "the run in flight is never forgotten");
        tx.send(()).unwrap();
        let (claim, handle) = filer.running.take().unwrap();
        let mut court_pending = Vec::new();
        filer.on_run_v1(claim, handle.await.unwrap(), 10_000, &own, &mut court_pending, &HashMap::new());
        assert!(court_pending.is_empty() && filer.settled_v1(&h(2)));
    }
}
