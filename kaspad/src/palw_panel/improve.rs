//! **RFC-0004's node loop (work item A10), the panel's child module** — past `palw_improvement_v1`, and
//! only there, the panel follows every governed line's open epoch and serves it:
//!
//! * **Watch.** Every [`PALW_IMPROVE_READ_EVERY_V1`] it reads the chain's doors (the open epochs, the
//!   evaluation view, the status), logs each epoch's state change once, and writes the status file an
//!   operator, a drill's watcher or a wallet reads (`<state-dir>/palw-improve-status.json`).
//! * **Prefetch** ([`PalwPanelService::improve_prefetch_v1`]). A seat holding a line's parent fetches a
//!   composite candidate's adapter section only (RFC-0004 §6.7): it scans the node's artifact drop
//!   directory (`--palw-improve-artifact-dir`) for the section whose reference is the candidate's
//!   (`parent_class`, `parent_root`, `adapter_root`, `P`), opens it over the held parent, checks it
//!   against the chain's reference, and holds the class — the registry's seat for it, a replay's weights.
//!   Full weights are never fetched unless the policy admits them and the operator opted in.
//! * **Evaluate** (`--palw-improve-evaluate`). It runs the evaluation jobs it can — every
//!   `(item, subject)` of an `Evaluating` epoch whose subject class it holds that no claim has taken —
//!   one at a time, off the loop (`palw_eval_run_v1`: the chain's context, the subject's weights), and
//!   carries the finished claim at the panel's own carrier site (an evaluation claim is an FP commitment
//!   whose job is version 9 and whose payload carries the ids and the score, signed by the node's bond,
//!   funded like every carrier from the fee float). The claim earns the job's evaluation fee at `Final`
//!   and no quantum, no ticket, no eligibility (MIP-17).
//! * **Judge** ([`PalwPanelService::improve_seat_step_v1`]). A seat drawn onto an evaluation claim
//!   (`free_prompt` with no quanta) replays it from the chain's state alone — the job, the item's
//!   prompt and reference, the policy's stage parameters — on its own weights and holds the replay's
//!   roots to the claim row's. A matching replay licenses `Valid`; a difference is the court's question
//!   (a seat files nothing on it, never a sampled conviction), and a claim this seat cannot judge is
//!   never accused of withholding: an evaluation claim has no material to withhold.
//!
//! * **Accuse** (`--palw-challenge`, [`PalwPanelService::improve_court_pass_v1`]). A challenger's audit of an
//!   evaluation claim whose replay differs locates the lie from the accused's capture and builds the evaluation court's
//!   proof in the same blocking task (`palw_eval_court_filing_v1`: `EvalCone` at the first divergent step leaf,
//!   `EvalDecodeToken` at a moved id or score, each checked as the chain checks it and offered only when it convicts); the
//!   pass signs it as the IR one-move accusation it is (`TirShardCourtAccused`, tag 62, the accuser's ML-DSA-87 over its
//!   session id) and queues it on the court's own carrier path. A leaf whose cone reduces over the history is only named
//!   (the held regime tries no such leaf in one move: the chain opens a dissection session there, and the accused's
//!   silence is the clock's to convict — this build plays no further move of it).
//!
//! **Dormant**: nothing here runs below the fence.

use super::*;

use std::collections::VecDeque;
use std::path::Path;

use kaspa_consensus_core::palw_improve_node_v1::{PalwImprovementEvalViewV1, PalwImprovementStatusV1};
use kaspa_consensus_core::palw_improve_state_v1::PalwImprovementEpochViewV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_one_move_v1::{
    PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1, PALW_TIR_ONE_MOVE_VERSION_V1, PalwTirOneMoveAccusationV1,
    palw_tir_one_move_session_id_v1, palw_tir_one_move_shape_v1,
};
use misaka_palw_sdk::improve::{
    PalwImproveDutyV1, PalwImproveEvalTaskV1, PalwImproveNodeChainV1, PalwImproveNodeV1, PalwImprovePrefetchV1,
    palw_improve_stage_params_of_v1,
};
use misaka_palw_sdk::improve_eval::{
    PalwEvalCaptureV1, PalwEvalClaimFactsV1, PalwEvalCommittedRootsV1, PalwEvalDisputeV1, PalwEvalFilingV1, PalwEvalHeldV1,
    PalwEvalSeatJudgmentV1, PalwEvalWorkV1, palw_eval_claim_v1, palw_eval_court_filing_v1, palw_eval_dispute_committed_v1,
    palw_eval_run_faulted_v1, palw_eval_run_v1, palw_eval_seat_judge_roots_v1,
};

use crate::palw_improve_watch::{
    PALW_IMPROVE_READ_EVERY_V1, PalwImproveDisputeV1, PalwImproveLieV1, PalwImproveWatchV1, palw_improve_admitting_classes_v1,
    palw_improve_disputes_json_v1, palw_improve_held_classes_v1, palw_improve_lie_carried_step_v1, palw_improve_log_tick_v1,
    palw_improve_status_json_v1, palw_improve_watch_armed_v1,
};

/// A job this node ran whose claim has not taken it is offered again after this many DAA (a carrier
/// the mempool lost, a claim another node's took first and then voided).
const PALW_IMPROVE_RETRY_AFTER_DAA_V1: u64 = 48;
/// Evaluation runs in flight at once: a run is a whole pipeline of a subject class.
const PALW_IMPROVE_MAX_RUNNING_V1: usize = 1;
/// A finished claim not carried within this many DAA is stale (its anchor ages, its epoch moves on).
const PALW_IMPROVE_READY_TTL_DAA_V1: u64 = 24;

/// One evaluation run in flight.
struct PalwImproveRunV1 {
    task: PalwImproveEvalTaskV1,
    handle: tokio::task::JoinHandle<Result<PalwEvalWorkV1, String>>,
}

/// **What the panel's improvement loop remembers between ticks.**
#[derive(Default)]
pub(super) struct PalwImproveLoopV1 {
    watch: PalwImproveWatchV1,
    read_at: Option<std::time::Instant>,
    /// The open epochs and the evaluation view as last read.
    views: Vec<PalwImprovementEpochViewV1>,
    evals: Vec<PalwImprovementEvalViewV1>,
    /// The IR classes this node holds, as evaluation subjects.
    held: HashMap<Hash64, Arc<PalwEvalHeldV1>>,
    held_ids: std::collections::BTreeSet<Hash64>,
    running: Vec<PalwImproveRunV1>,
    /// Finished runs waiting for a carrier slot, with the DAA they finished at.
    ready: VecDeque<(PalwEvalWorkV1, u64)>,
    /// The job ids this node ran, and when.
    attempted: HashMap<Hash64, u64>,
    /// Audits of evaluation claims — a drawn seat's replay, or a challenger's — in flight and done.
    seat_runs: HashMap<Hash64, tokio::task::JoinHandle<PalwImproveAuditV1>>,
    seat_done: HashMap<Hash64, PalwEvalSeatJudgmentV1>,
    /// The disputes this node found (RFC-0004 D-M3): the claims whose replay on its weights differs, by claim id.
    disputes: std::collections::BTreeMap<Hash64, PalwImproveDisputeV1>,
    /// Accusations an audit built that wait for [`PalwPanelService::improve_court_pass_v1`] to sign and queue them.
    court_ready: Vec<(Hash64, PalwEvalFilingV1)>,
    /// The claims whose accusation this node has queued, or found there is none to file: once each.
    court_done: HashSet<Hash64>,
    /// Where this node's drill lie stands (`--palw-drill-tamper-eval`): told until one lands on the chain.
    lie: PalwImproveLieV1,
    /// The DAA of the last tick (what a finding is dated by).
    daa: u64,
    /// The section files this node already tried against a candidate (by path and modification time).
    sections_tried: HashSet<(PathBuf, u64)>,
    /// Jobs this node ran whose claim came out past the job's share of the epoch's position budget
    /// (MIP-20): the chain would refuse it whole, so it is neither carried nor run again.
    over_budget: HashSet<Hash64>,
}

/// **What an audit of an evaluation claim came to**: the judgment of its replay and, when that differs and the
/// accused's capture was served, where the capture parts from the honest run — and, for a challenger, the court's proof
/// built from it.
struct PalwImproveAuditV1 {
    judgment: PalwEvalSeatJudgmentV1,
    dispute: Option<Result<PalwEvalDisputeV1, String>>,
    /// The accusation's proof (`None`: this audit builds none — the node is no challenger, or there is no located dispute).
    filing: Option<Result<PalwEvalFilingV1, String>>,
}

/// **What a challenger builds an accusation under**: the ruleset's court (the work limits and the cost ceiling a close is
/// held to) and whether the chain is in the held regime (a dissected leaf is then only challenged).
#[derive(Clone, Copy)]
struct PalwImproveCourtRulesV1 {
    court: PalwCourtParamsV2,
    held_regime: bool,
}

/// **Replay an evaluation claim, and dispute it from the accused's capture when it differs.** The capture is
/// read from `capture` and held to the roots the claim committed ([`palw_eval_dispute_committed_v1`]): a file
/// that is not the accused's disputes nothing. With `court` (a challenger), a located dispute is also built into the
/// proof that convicts ([`palw_eval_court_filing_v1`]) and held to the court's cost ceiling.
fn palw_improve_audit_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    roots: &PalwEvalCommittedRootsV1,
    capture: &Path,
    court: Option<PalwImproveCourtRulesV1>,
) -> PalwImproveAuditV1 {
    let judgment = palw_eval_seat_judge_roots_v1(held, task, roots);
    let mut filing = None;
    let dispute = matches!(judgment, PalwEvalSeatJudgmentV1::Differs(_)).then(|| {
        let bytes = std::fs::read(capture).map_err(|e| format!("no capture served for the claim ({}): {e}", capture.display()))?;
        let accused =
            borsh::from_slice::<PalwEvalCaptureV1>(&bytes).map_err(|e| format!("the served capture does not decode: {e}"))?;
        let found = palw_eval_dispute_committed_v1(held, task, roots, &accused)?;
        if let Some(rules) = court
            && !matches!(found, PalwEvalDisputeV1::Agrees)
        {
            let limits = kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&rules.court);
            filing = Some(palw_eval_court_filing_v1(held, task, roots, &accused, &found, &limits, rules.held_regime).and_then(|f| {
                kaspa_consensus_core::palw_court_v2::check_close_cost_v2(&f.proof, &rules.court)
                    .map_err(|e| format!("the close is over the court's cost ceiling: {e}"))?;
                Ok(f)
            }));
        }
        Ok::<PalwEvalDisputeV1, String>(found)
    });
    PalwImproveAuditV1 { judgment, dispute, filing }
}

/// What a seat's step on an evaluation duty came to.
pub(super) enum PalwImproveSeatStepV1 {
    /// Not an evaluation claim: the panel's other arms judge it.
    NotEvaluation,
    /// The replay is running, or the chain's view does not hold the claim yet: nothing to file now.
    Waiting,
    /// The replay reproduced the claim.
    Valid,
    /// The replay differs: the court's question — nothing is filed (the reason is logged).
    Differs,
    /// This seat cannot judge the claim (a class it does not hold, a job it cannot derive): nothing is
    /// filed, and in particular no `Unavailable` (the reason is logged).
    Unjudgeable,
}

/// A finished judgment as the seat's step.
fn palw_improve_seat_step_of_v1(judgment: &PalwEvalSeatJudgmentV1) -> PalwImproveSeatStepV1 {
    match judgment {
        PalwEvalSeatJudgmentV1::Valid => PalwImproveSeatStepV1::Valid,
        PalwEvalSeatJudgmentV1::Differs(_) => PalwImproveSeatStepV1::Differs,
        PalwEvalSeatJudgmentV1::Unjudgeable(_) => PalwImproveSeatStepV1::Unjudgeable,
    }
}

fn palw_improve_mtime_v1(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

impl PalwPanelService {
    /// Is this node a candidate evaluator: it asked to (`--palw-improve-evaluate`) and has a bond and a
    /// key to carry the claims under.
    fn improve_evaluates_v1(&self) -> bool {
        self.config.improve_evaluate && self.bond.is_some() && self.keypair.is_some()
    }

    /// **The IR classes this node holds, as evaluation subjects** — rebuilt when its holdings change.
    fn improve_refresh_held_v1(&self, st: &mut PalwImproveLoopV1) {
        let backends = self.backends();
        let holds = palw_improve_held_classes_v1(backends.holdings());
        if holds == st.held_ids {
            return;
        }
        let mut held = HashMap::new();
        for entry in misaka_palw_sdk::tir_registration::tir_entries_of_v1(backends.holdings()) {
            match PalwEvalHeldV1::from_entry(&entry) {
                Ok(h) => {
                    held.insert(h.class_id, Arc::new(h));
                }
                Err(why) => warn!("[{PALW_PANEL}] [palw-improve] class {} cannot serve evaluations: {why}", entry.class_id()),
            }
        }
        st.held = held;
        st.held_ids = holds;
    }

    /// **One tick of the improvement loop** (cheap: the reads are every [`PALW_IMPROVE_READ_EVERY_V1`],
    /// the runs off the loop). Past `palw_improvement_v1` only.
    pub(super) async fn improve_tick_v1(&self, st: &mut PalwImproveLoopV1, current_daa: u64) {
        if !palw_improve_watch_armed_v1(&self.consensus_config.params, current_daa) {
            return;
        }
        st.daa = current_daa;
        self.improve_reap_v1(st, current_daa).await;
        self.improve_audit_reap_v1(st).await;
        self.improve_lie_tick_v1(st, current_daa);
        if st.read_at.is_some_and(|at| at.elapsed() < PALW_IMPROVE_READ_EVERY_V1) {
            return;
        }
        st.read_at = Some(std::time::Instant::now());
        let (views, evals, status, admitting) = self
            .consensus_manager
            .consensus()
            .unguarded_session()
            .spawn_blocking(|c| {
                (
                    c.palw_improvement_open_epochs_v1(),
                    c.palw_improvement_eval_views_v1(),
                    c.palw_improvement_status_v1(),
                    palw_improve_admitting_classes_v1(c.palw_model_registry_v1().as_ref()),
                )
            })
            .await;
        self.improve_refresh_held_v1(st);
        self.improve_prefetch_recorded_v1(st, &status.composite_classes);
        let node = PalwImproveNodeV1 {
            holds: st.held_ids.clone(),
            evaluates: self.improve_evaluates_v1(),
            prefetch_full: false,
            admitting,
            ceilings: self.consensus_config.params.palw_improvement_v1.map(|fence| fence.ceilings),
        };
        let tick = {
            let chain = PalwImproveNodeChainV1 { views: &views, eval: &evals };
            st.watch.tick(&chain, &node, current_daa)
        };
        palw_improve_log_tick_v1(&tick);
        st.views = views;
        st.evals = evals;
        let disputes: Vec<PalwImproveDisputeV1> = st.disputes.values().cloned().collect();
        self.improve_write_status_v1(&status, &st.evals, &disputes, current_daa);
        // The retry rule: a job this node ran stays its own until the chain takes it or the retry age.
        st.attempted.retain(|_, at| current_daa < at.saturating_add(PALW_IMPROVE_RETRY_AFTER_DAA_V1));
        st.ready.retain(|(_, at)| current_daa < at.saturating_add(PALW_IMPROVE_READY_TTL_DAA_V1));
        for duty in tick.duties {
            match duty {
                PalwImproveDutyV1::Prefetch { line_id, epoch, class_id, plan } => {
                    self.improve_prefetch_v1(st, line_id, epoch, class_id, plan);
                }
                PalwImproveDutyV1::Evaluate { task, until_daa } => {
                    if st.running.len() + st.ready.len() >= PALW_IMPROVE_MAX_RUNNING_V1
                        || st.attempted.contains_key(&task.job_id)
                        || st.over_budget.contains(&task.job_id)
                    {
                        continue;
                    }
                    let Some(held) = st.held.get(&task.subject_class).cloned() else { continue };
                    info!(
                        "[{PALW_PANEL}] [palw-improve] evaluating item {} of epoch {} for {:?} ({:?}), due by DAA {until_daa}",
                        task.item, task.epoch, task.subject, task.kind
                    );
                    // **A drill's lie** (`--palw-drill-tamper-eval`, salted drill chains only): an evaluation of the named line
                    // (and subject) this node runs is committed with the fault — one lie in flight at a time, again on the next job
                    // while the last was lost to a racing claim, never once the chain holds one.
                    let fault = self
                        .config
                        .improve_tamper
                        .as_ref()
                        .filter(|t| st.lie == PalwImproveLieV1::Idle && t.applies_to(&task.line_id, &task.subject))
                        .map(|t| t.fault);
                    if let Some(fault) = fault {
                        st.lie = PalwImproveLieV1::Pending { job: task.job_id };
                        warn!(
                            "[{PALW_PANEL}] [palw-improve] DRILL: this node LIES about item {} of epoch {} of line {} for {:?}: {} \
                             (--palw-drill-tamper-eval) — the claim it files is self-consistent and an honest replay disputes it",
                            task.item,
                            task.epoch,
                            task.line_id,
                            task.subject,
                            fault.describe()
                        );
                    }
                    st.attempted.insert(task.job_id, current_daa);
                    let run = task.clone();
                    st.running.push(PalwImproveRunV1 {
                        task,
                        handle: tokio::task::spawn_blocking(move || match fault {
                            Some(fault) => palw_eval_run_faulted_v1(&held, &run, fault),
                            None => palw_eval_run_v1(&held, &run),
                        }),
                    });
                }
            }
        }
        self.improve_audit_tick_v1(st);
    }

    /// **Where the drill lie stands, against the chain** (see [`PalwImproveLieV1`]): a faulted run that is gone without a carrier is a lie
    /// not told; a carried one is spent when the chain's view holds it, lost when another claim took its job or none shows in time.
    fn improve_lie_tick_v1(&self, st: &mut PalwImproveLoopV1, daa: u64) {
        match st.lie {
            PalwImproveLieV1::Pending { job } => {
                let alive =
                    st.running.iter().any(|run| run.task.job_id == job) || st.ready.iter().any(|(work, _)| work.task.job_id == job);
                if !alive {
                    info!(
                        "[{PALW_PANEL}] [palw-improve] DRILL: the faulted run of job {job} is gone without a carrier: the lie is told again"
                    );
                    st.lie = PalwImproveLieV1::Idle;
                }
            }
            PalwImproveLieV1::Carried { job, claim, daa: carried } => {
                let claim_of_job = st
                    .evals
                    .iter()
                    .flat_map(|view| view.jobs.iter())
                    .find(|j| j.job.id() == job)
                    .and_then(|j| j.claim.as_ref().map(|c| c.claim_id));
                let next = palw_improve_lie_carried_step_v1(job, claim, carried, claim_of_job, daa);
                if next != st.lie {
                    match next {
                        PalwImproveLieV1::Spent => {
                            info!(
                                "[{PALW_PANEL}] [palw-improve] DRILL: the lying claim {claim} landed on the chain: this node is honest from here"
                            )
                        }
                        _ => info!(
                            "[{PALW_PANEL}] [palw-improve] DRILL: the lying claim {claim} did not land (its job was taken, or it never mined): \
                             the lie is told again on the next job"
                        ),
                    }
                    st.lie = next;
                }
            }
            PalwImproveLieV1::Idle | PalwImproveLieV1::Spent => {}
        }
    }

    /// **Prefetch every composite class the chain records and this node does not hold** (RFC-0004 §6.7): the epoch's plan
    /// fetches a candidate's section while its epoch is open, but a promoted composite is the line's head long after, and a
    /// seat that restarted — or whose section was dropped late — must hold it to be a ready seat for it (the parent clause)
    /// and to replay the claims of its line. The record outlives the epoch row, so it is the record that names the section.
    fn improve_prefetch_recorded_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        records: &[(Hash64, kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1)],
    ) {
        for (class_id, r) in records {
            if st.held_ids.contains(class_id) {
                continue;
            }
            let plan = PalwImprovePrefetchV1::Adapter {
                parent_class: r.parent_class,
                parent_root: r.parent_root,
                adapter_root: r.adapter_root,
                p: r.p,
            };
            self.improve_prefetch_v1(st, Hash64::default(), 0, *class_id, plan);
        }
    }

    /// **Where an evaluation claim's capture is retained and read from**: `--palw-improve-capture-dir`, else beside
    /// the node's other retention. An executor writes the capture of every claim it carries under `<claim id>.capture`;
    /// a challenger reads the accused's from the same directory. That shared directory is the evidence transport of a
    /// drill on one machine (the chain's data-availability units for a pipeline claim are lane B's and lane A's).
    fn improve_capture_dir_v1(&self) -> PathBuf {
        self.config.improve_capture_dir.clone().unwrap_or_else(|| self.config.retention_dir.join("improve"))
    }

    /// Move the finished runs on: a run that failed is said once and left to the retry age.
    async fn improve_reap_v1(&self, st: &mut PalwImproveLoopV1, current_daa: u64) {
        let mut i = 0;
        while i < st.running.len() {
            if !st.running[i].handle.is_finished() {
                i += 1;
                continue;
            }
            let run = st.running.remove(i);
            match run.handle.await {
                Ok(Ok(work)) => st.ready.push_back((work, current_daa)),
                Ok(Err(why)) => warn!(
                    "[{PALW_PANEL}] [palw-improve] the evaluation of item {} of epoch {} for {:?} failed: {why}",
                    run.task.item, run.task.epoch, run.task.subject
                ),
                Err(e) => warn!("[{PALW_PANEL}] [palw-improve] an evaluation run did not finish: {e}"),
            }
        }
    }

    /// **The status file**: the chain's improvement status at the tip, as JSON, beside the panel's other
    /// state. Written whole then renamed, so a reader never sees half a file.
    fn improve_write_status_v1(
        &self,
        status: &PalwImprovementStatusV1,
        evals: &[PalwImprovementEvalViewV1],
        disputes: &[PalwImproveDisputeV1],
        daa: u64,
    ) {
        let path = self.config.state_dir.join("palw-improve-status.json");
        let mut json = palw_improve_status_json_v1(status, evals, daa);
        json["disputes"] = palw_improve_disputes_json_v1(disputes);
        let _ = std::fs::create_dir_all(&self.config.state_dir);
        let partial = path.with_extension("json.partial");
        if let Err(e) = std::fs::write(&partial, serde_json::to_vec_pretty(&json).unwrap_or_default())
            .and_then(|()| std::fs::rename(&partial, &path))
        {
            crate::palw_backends::note_throttled_v1("improve-status-file", || {
                format!("[{PALW_PANEL}] [palw-improve] cannot write {}: {e}", path.display())
            });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Prefetch
    // -----------------------------------------------------------------------------------------

    /// **Prefetch a candidate's artifact** (RFC-0004 §6.7). A composite candidate over a parent this node
    /// holds is its adapter section only: the node scans its drop directory for the `PALWTIRS` file whose
    /// record is the candidate's reference, opens it over the held parent (the artifact checks the
    /// composite rule, the parent's root and the adapter root), and holds the class it declares if that
    /// class is the candidate's. Full weights come the same way as a `PALWTIR1` file named by the
    /// candidate's root, only where the plan said so.
    fn improve_prefetch_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        line_id: Hash64,
        epoch: u64,
        class_id: Hash64,
        plan: PalwImprovePrefetchV1,
    ) {
        let Some(dir) = self.config.improve_artifact_dir.as_deref() else {
            crate::palw_backends::note_throttled_v1("improve-prefetch-no-dir", || {
                format!(
                    "[{PALW_PANEL}] [palw-improve] candidate {class_id} of epoch {epoch} of line {line_id} wants prefetching, and this \
                     node has no --palw-improve-artifact-dir to find its artifact in"
                )
            });
            return;
        };
        let Ok(read) = std::fs::read_dir(dir) else { return };
        let mut files: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
        files.sort();
        for path in files {
            let key = (path.clone(), palw_improve_mtime_v1(&path));
            if st.sections_tried.contains(&key) {
                continue;
            }
            // Only the section whose record is this candidate's reference is opened at all.
            let wanted = match plan {
                PalwImprovePrefetchV1::Adapter { parent_class, parent_root, adapter_root, p } => {
                    let Some(Ok(r)) = misaka_palw_sdk::tir_composite::tir_composite_ref_of_path_v1(&path) else { continue };
                    r.parent_class == parent_class && r.parent_root == parent_root && r.adapter_root == adapter_root && r.p == p
                }
                PalwImprovePrefetchV1::Full { .. } => path.extension().is_some_and(|x| x == "palwtir"),
            };
            if !wanted {
                continue;
            }
            // The parent this node holds, from whichever loader mapped it (the process-wide cache hands one
            // out): the section opens over that entry, not over a lineage's own list.
            let PalwImprovePrefetchV1::Adapter { parent_class, parent_root, .. } = plan else {
                st.sections_tried.insert(key);
                warn!("[{PALW_PANEL}] [palw-improve] full-weight prefetch of {} is not served by this build", path.display());
                return;
            };
            let holdings = self.backends().holdings().to_vec();
            let Some(parent) = misaka_palw_sdk::tir_registration::tir_entries_of_v1(&holdings)
                .into_iter()
                .find(|e| e.artifact.composite_ref().is_none() && e.class_id() == parent_class && e.artifact_root == parent_root)
            else {
                // Not marked tried: a parent held later opens it.
                crate::palw_backends::note_throttled_v1("improve-prefetch-parent", || {
                    format!(
                        "[{PALW_PANEL}] [palw-improve] the parent {parent_class} of candidate {class_id} is not held: nothing to open {} over",
                        path.display()
                    )
                });
                return;
            };
            st.sections_tried.insert(key);
            match misaka_palw_sdk::lineages::tir::TirLineageV1::load_composite_over(&parent, &path) {
                Ok(loaded) => {
                    let declared: Vec<Hash64> = misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&loaded))
                        .iter()
                        .map(|e| e.class_id())
                        .collect();
                    if declared.contains(&class_id) {
                        info!(
                            "[{PALW_PANEL}] [palw-improve] prefetched candidate {class_id} of epoch {epoch} of line {line_id} from {} — \
                             this seat now holds the class",
                            path.display()
                        );
                        self.improve_holdings.lock().expect("the prefetched holdings are never poisoned").push(loaded);
                    } else {
                        warn!(
                            "[{PALW_PANEL}] [palw-improve] {} opens over the held parent but declares another class than the candidate \
                             {class_id}: not held",
                            path.display()
                        );
                    }
                }
                Err(why) => {
                    warn!("[{PALW_PANEL}] [palw-improve] {} is not the candidate {class_id}'s artifact: {why}", path.display())
                }
            }
            return;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Carrying a claim
    // -----------------------------------------------------------------------------------------

    /// **Build, sign and fund an evaluation claim's transaction** — the facts the chain asks of any
    /// free-prompt commitment (a fresh anchor, the bond's operator, the retention), read at the moment of
    /// carrying, the run's claim assembled and checked as the chain's door checks it, the carrier priced
    /// from its own compute mass.
    fn improve_build_claim_tx_v1(
        &self,
        st: &PalwImproveLoopV1,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        bond: TransactionOutpoint,
        current_daa: u64,
        work: &PalwEvalWorkV1,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
    ) -> Result<(Transaction, Hash64), String> {
        const EVALUATION_CLAIM_FEE_FLOOR_SOMPI: u64 = 250_000;
        let held = st.held.get(&work.task.subject_class).ok_or("this node no longer holds the subject class")?;
        let base = session
            .palw_v2_class_table()
            .into_iter()
            .find(|row| row.is_base_class)
            .map(|row| row.class_id)
            .ok_or("the chain names no base class to read the producer's facts under")?;
        let facts = session.palw_producer_facts_v2(base, Some(bond)).ok_or("no producer facts")?;
        let bond_facts = facts.bond.as_ref().ok_or("this bond is not registered on the chain")?;
        let kp = self.keypair.as_ref().ok_or("no signing key")?;
        let claim_facts = PalwEvalClaimFactsV1 {
            network_domain,
            executor_bond: bond,
            executor_pubkey: kp.verification_key.as_ref().to_vec(),
            operator_id: bond_facts.operator_id,
            anchor_block: facts.chain_point,
            anchor_daa: facts.daa_score,
            prompt_ids_form: self.class_prompt_ids_form(work.task.subject_class),
            trace_retention_daa: current_daa.saturating_add(facts.min_trace_retention_daa),
        };
        let claim = palw_eval_claim_v1(work, held, &claim_facts)?;
        let claim_id = claim.claim_id();
        let seed = kaspa_pq_validator_core::load_validator_seed(&self.config.key_path)?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        let build = |fee: u64| {
            key.build_fp_eval_commitment_tx(
                claim.commitment.clone(),
                &claim.tail,
                claim.prompt.clone(),
                funding_outpoint,
                funding,
                fee,
            )
        };
        let probe = build(EVALUATION_CLAIM_FEE_FLOOR_SOMPI)?;
        let params = &self.consensus_config.params;
        let mass_calculator = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let fee = relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&probe).compute_mass)
            .max(EVALUATION_CLAIM_FEE_FLOOR_SOMPI);
        Ok((build(fee)?, claim_id))
    }

    /// **A task's job share of its epoch's evaluation budget** (MIP-20), from the epoch views as last read
    /// and the network's ceilings; `None` where either is unknown.
    fn improve_job_cap_of_v1(&self, st: &PalwImproveLoopV1, task: &PalwImproveEvalTaskV1) -> Option<u64> {
        let ceilings = self.consensus_config.params.palw_improvement_v1.map(|fence| fence.ceilings)?;
        let view = st.views.iter().find(|v| v.line.line_id == task.line_id && v.epoch.epoch == task.epoch)?;
        let epoch = misaka_palw_sdk::improve::PalwImproveEpochViewV1::of_view_v1(view);
        Some(misaka_palw_sdk::improve::palw_improve_job_cap_v1(&view.policy, &epoch, &ceilings))
    }

    /// **Carry one finished evaluation claim at the panel's carrier site**, funded and chained exactly as
    /// the canonical claim is: `true` when a carrier went out (the caller's slot count and funding
    /// follow), `false` when nothing was carried (nothing finished, no funding, or the build refused).
    pub(super) async fn improve_carry_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        bond: TransactionOutpoint,
        current_daa: u64,
        funding: &mut Option<(TransactionOutpoint, UtxoEntry)>,
        inflight: &mut usize,
    ) -> bool {
        let Some((funding_outpoint, funding_entry)) = funding.clone() else { return false };
        let Some((work, finished_daa)) = st.ready.pop_front() else { return false };
        // The drill lie follows its claim: if this is the faulted run, a claim not carried is a lie not told.
        let lying = matches!(st.lie, PalwImproveLieV1::Pending { job } if job == work.task.job_id);
        // A claim another node's took meanwhile is not carried: the first valid claim per job is the one.
        let taken = {
            let chain = PalwImproveNodeChainV1 { views: &st.views, eval: &st.evals };
            misaka_palw_sdk::improve::PalwImproveChainV1::job_claimed(&chain, &work.task.job_id)
        };
        if taken {
            info!("[{PALW_PANEL}] [palw-improve] job {} is taken already: not carrying this node's claim", work.task.job_id);
            if lying {
                st.lie = PalwImproveLieV1::Idle;
            }
            return false;
        }
        // MIP-20: the chain refuses a claim past its job's share of the epoch's evaluation budget whole, so
        // a run that came out over it (a generation that ran long) is not carried.
        if let Some(cap) = self.improve_job_cap_of_v1(st, &work.task) {
            let positions = work.task.positions_of(work.tail.generated.len());
            if positions > cap {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] item {} of epoch {} for {:?} took {positions} positions, past its job's share of the \
                     epoch's evaluation budget ({cap}): the chain would refuse the claim, so it is not carried",
                    work.task.item, work.task.epoch, work.task.subject
                );
                if st.over_budget.len() >= 4096 {
                    st.over_budget.clear();
                }
                st.over_budget.insert(work.task.job_id);
                if lying {
                    st.lie = PalwImproveLieV1::Idle;
                }
                return false;
            }
        }
        let (tx, claim_id) = match self.improve_build_claim_tx_v1(
            st,
            session,
            network_domain,
            bond,
            current_daa,
            &work,
            funding_outpoint,
            &funding_entry,
        ) {
            Ok(built) => built,
            Err(why) => {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] cannot build the claim for item {} of epoch {} ({:?}): {why}",
                    work.task.item, work.task.epoch, work.task.subject
                );
                if lying {
                    st.lie = PalwImproveLieV1::Idle;
                }
                return false;
            }
        };
        let txid = tx.id();
        let change = tx.outputs[0].clone();
        match self.flow_context.submit_rpc_transaction(session, tx, Orphan::Forbidden).await {
            Ok(()) => {
                info!(
                    "[{PALW_PANEL}] [palw-improve] carried evaluation claim {claim_id} in tx {txid}: item {} of epoch {} for {:?} \
                     ({:?}), {} leaves (finished at DAA {finished_daa})",
                    work.task.item,
                    work.task.epoch,
                    work.task.subject,
                    work.task.kind,
                    work.execution.space.leaf_count()
                );
                // The capture, retained under the claim id: what data availability will serve and a
                // challenger rebuilds an accused's execution from. Best effort, bounded.
                if let Some(held) = st.held.get(&work.task.subject_class)
                    && work.execution.space.leaf_count() <= PALW_IMPROVE_CAPTURE_MAX_LEAVES_V1
                    && let Ok(capture) = PalwEvalCaptureV1::of(&work, held)
                    && let Ok(bytes) = borsh::to_vec(&capture)
                {
                    let dir = self.improve_capture_dir_v1();
                    let file = dir.join(format!("{claim_id}.capture"));
                    let partial = dir.join(format!("{claim_id}.capture.partial"));
                    if std::fs::create_dir_all(&dir)
                        .and_then(|()| std::fs::write(&partial, &bytes))
                        .and_then(|()| std::fs::rename(&partial, &file))
                        .is_err()
                    {
                        warn!("[{PALW_PANEL}] [palw-improve] cannot retain the capture of claim {claim_id}");
                    }
                }
                if lying {
                    st.lie = PalwImproveLieV1::Carried { job: work.task.job_id, claim: claim_id, daa: current_daa };
                }
                let next = TransactionOutpoint::new(txid, 0);
                self.persist_fee_outpoint(next);
                *funding = Some((
                    next,
                    UtxoEntry {
                        amount: change.value,
                        script_public_key: change.script_public_key,
                        block_daa_score: current_daa,
                        is_coinbase: false,
                    },
                ));
                *inflight += 1;
                true
            }
            Err(e) => {
                warn!("[{PALW_PANEL}] [palw-improve] the mempool refused the evaluation claim: {e}");
                // The job is offered again at the retry age; the funding outpoint may be stale.
                *funding = None;
                if lying {
                    st.lie = PalwImproveLieV1::Idle;
                }
                false
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The seat
    // -----------------------------------------------------------------------------------------

    /// **The task a claim's job derives, from the chain's state alone**: the job and the claim's roots
    /// the evaluation view holds, the item's prompt and reference, the policy's stage parameters — what a
    /// seat replays. `Err` says why not (the view does not hold the claim, or the item's material).
    fn improve_task_of_claim_v1(
        st: &PalwImproveLoopV1,
        claim_id: &Hash64,
    ) -> Result<(PalwImproveEvalTaskV1, PalwEvalCommittedRootsV1), &'static str> {
        let (view, job) = st
            .evals
            .iter()
            .find_map(|v| v.jobs.iter().find(|j| j.claim.as_ref().is_some_and(|c| c.claim_id == *claim_id)).map(|j| (v, j)))
            .ok_or("the chain's evaluation view does not hold the claim")?;
        let claim = job.claim.as_ref().ok_or("the job holds no claim")?;
        let epoch =
            st.views.iter().find(|e| e.line.line_id == view.line_id && e.epoch.epoch == view.epoch).ok_or("the epoch is not open")?;
        let item_view = view.items.iter().find(|i| i.item == job.job.item).ok_or("the item's material is not on chain")?;
        let params =
            palw_improve_stage_params_of_v1(&epoch.policy, job.job.kind).ok_or("the policy has no stage of the job's kind")?;
        let subject_class = match job.job.subject {
            kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1::Parent => epoch.epoch.parent,
            kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1::Candidate(c)
            | kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1::Previous(c) => c,
        };
        let task = PalwImproveEvalTaskV1 {
            line_id: job.job.line_id,
            epoch: job.job.epoch,
            item: job.job.item,
            subject: job.job.subject,
            subject_class,
            kind: job.job.kind,
            mode: job.job.mode.clone(),
            job_id: job.job.id(),
            prompt_ids: item_view.prompt_ids.clone().ok_or("the item's prompt is not disclosed")?,
            reference_ids: match job.job.mode {
                kaspa_consensus_core::palw_improve_eval_v1::PalwEvalModeV1::TeacherForced { .. } => {
                    item_view.reference_ids.clone().ok_or("the item's reference is not disclosed")?
                }
                _ => Vec::new(),
            },
            params,
        };
        let roots = PalwEvalCommittedRootsV1 {
            trace_root: claim.trace_root,
            output_root: claim.output_root,
            execution_root: claim.execution_root,
            work_leaves: claim.work_leaves,
        };
        Ok((task, roots))
    }

    /// **A seat's step on a duty**: `NotEvaluation` for every claim but an evaluation claim (a
    /// free-prompt claim with no quanta); for one, its replay — started off the loop on the first step,
    /// polled on the next — and its judgment. A judgment is final for the claim; a replay that cannot run
    /// is said once.
    pub(super) async fn improve_seat_step_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    ) -> PalwImproveSeatStepV1 {
        if !(duty.free_prompt && duty.quanta == 0) {
            return PalwImproveSeatStepV1::NotEvaluation;
        }
        let claim = duty.claim_id;
        if let Some(done) = st.seat_done.get(&claim) {
            return palw_improve_seat_step_of_v1(done);
        }
        if st.seat_runs.get(&claim).is_some() {
            if !st.seat_runs[&claim].is_finished() {
                return PalwImproveSeatStepV1::Waiting;
            }
            let judgment = self.improve_audit_finish_v1(st, claim).await;
            return palw_improve_seat_step_of_v1(&judgment);
        }
        match self.improve_audit_start_v1(st, claim) {
            Ok(()) => PalwImproveSeatStepV1::Waiting,
            // The view moves with the chain: wait for it (it is read every few seconds), never answer.
            Err(PalwImproveAuditWaitV1::ChainView) => PalwImproveSeatStepV1::Waiting,
            Err(PalwImproveAuditWaitV1::NotHeld) => PalwImproveSeatStepV1::Unjudgeable,
        }
    }

    /// **Start the audit of an evaluation claim** — its replay off the loop, on this node's weights, from the chain's
    /// state alone — unless the chain's view does not hold it yet or this node does not hold its class.
    fn improve_audit_start_v1(&self, st: &mut PalwImproveLoopV1, claim: Hash64) -> Result<(), PalwImproveAuditWaitV1> {
        let (task, roots) = match Self::improve_task_of_claim_v1(st, &claim) {
            Ok(derived) => derived,
            Err(why) => {
                crate::palw_backends::note_throttled_v1("improve-seat-waits", || {
                    format!("[{PALW_PANEL}] [palw-improve] claim {claim}: not judged yet — {why}")
                });
                return Err(PalwImproveAuditWaitV1::ChainView);
            }
        };
        let Some(held) = st.held.get(&task.subject_class).cloned() else {
            crate::palw_backends::note_throttled_v1("improve-seat-unheld", || {
                format!("[{PALW_PANEL}] [palw-improve] claim {claim}: this seat does not hold class {}", task.subject_class)
            });
            return Err(PalwImproveAuditWaitV1::NotHeld);
        };
        info!(
            "[{PALW_PANEL}] [palw-improve] replaying evaluation claim {claim}: item {} of epoch {} for {:?} ({:?})",
            task.item, task.epoch, task.subject, task.kind
        );
        let capture = self.improve_capture_dir_v1().join(format!("{claim}.capture"));
        let court = self.improve_court_rules_v1(st.daa);
        st.seat_runs.insert(claim, tokio::task::spawn_blocking(move || palw_improve_audit_v1(&held, &task, &roots, &capture, court)));
        Ok(())
    }

    /// **What this node builds an evaluation accusation under**, `None` where it files none: it is no challenger
    /// (`--palw-challenge`), has no bond and key to accuse under, or the network has no V2 court or no IR court yet. A seat's
    /// audit builds none either way: a seat files nothing on a differing replay.
    fn improve_court_rules_v1(&self, daa: u64) -> Option<PalwImproveCourtRulesV1> {
        if !self.config.challenge || self.bond.is_none() || self.keypair.is_none() {
            return None;
        }
        let params = &self.consensus_config.params;
        if !matches!(params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
            || !params.palw_tir_v1_active_at(daa)
        {
            return None;
        }
        Some(PalwImproveCourtRulesV1 { court: self.config.court, held_regime: params.palw_held_context_active_at(daa) })
    }

    /// **Collect a finished audit.** A judgment is final for the claim; a replay that differs is the court's
    /// question and is recorded as a dispute — where it parts from the honest run, when the accused's capture was
    /// served — a replay that cannot run is said once.
    async fn improve_audit_finish_v1(&self, st: &mut PalwImproveLoopV1, claim: Hash64) -> PalwEvalSeatJudgmentV1 {
        let handle = st.seat_runs.remove(&claim).expect("a finished audit's handle is held");
        let audit = handle.await.unwrap_or_else(|e| PalwImproveAuditV1 {
            judgment: PalwEvalSeatJudgmentV1::Unjudgeable(format!("the replay did not finish: {e}")),
            dispute: None,
            filing: None,
        });
        match &audit.judgment {
            PalwEvalSeatJudgmentV1::Valid => info!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: the replay reproduces it"),
            PalwEvalSeatJudgmentV1::Differs(why) => {
                warn!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: the replay differs — the court's question: {why}");
                let files = self.improve_court_rules_v1(st.daa).is_some();
                self.improve_note_dispute_v1(st, claim, why, audit.dispute, &audit.filing, files);
                match audit.filing {
                    Some(Ok(filing)) => st.court_ready.push((claim, filing)),
                    Some(Err(_)) | None => {
                        st.court_done.insert(claim);
                    }
                }
            }
            PalwEvalSeatJudgmentV1::Unjudgeable(why) => {
                warn!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: cannot be judged: {why}")
            }
        }
        st.seat_done.insert(claim, audit.judgment.clone());
        audit.judgment
    }

    /// Collect every audit that finished since the last tick (a challenger's, or a seat's that no duty polled).
    async fn improve_audit_reap_v1(&self, st: &mut PalwImproveLoopV1) {
        let finished: Vec<Hash64> = st.seat_runs.iter().filter(|(_, handle)| handle.is_finished()).map(|(claim, _)| *claim).collect();
        for claim in finished {
            self.improve_audit_finish_v1(st, claim).await;
        }
    }

    /// **The challenger's pass** (`--palw-challenge`): one at a time, replay an evaluation claim this node did not
    /// make that is not yet final and not void — whether or not a seat duty names it — so that a lie is found by
    /// whoever holds the subject class. A drawn seat's replay is the same audit.
    fn improve_audit_tick_v1(&self, st: &mut PalwImproveLoopV1) {
        if !self.config.challenge || st.seat_runs.len() >= PALW_IMPROVE_MAX_RUNNING_V1 {
            return;
        }
        let own = self.bond;
        let candidates: Vec<Hash64> = st
            .evals
            .iter()
            .flat_map(|view| view.jobs.iter())
            .filter_map(|job| job.claim.as_ref())
            .filter(|claim| {
                !claim.voided
                    && claim.final_daa.is_none()
                    && own.is_none_or(|bond| claim.bond.0 != bond)
                    && !st.seat_done.contains_key(&claim.claim_id)
                    && !st.seat_runs.contains_key(&claim.claim_id)
            })
            .map(|claim| claim.claim_id)
            .collect();
        for claim in candidates {
            if self.improve_audit_start_v1(st, claim).is_ok() {
                break;
            }
        }
    }

    /// **Record a dispute** ([`PalwImproveDisputeV1`]): what the replay found, where the accused's capture says the
    /// lie is, and how the dispute stands — the accusation built for it (`filing`, a challenger's), or why none is.
    fn improve_note_dispute_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        claim: Hash64,
        why: &str,
        found: Option<Result<PalwEvalDisputeV1, String>>,
        filing: &Option<Result<PalwEvalFilingV1, String>>,
        files: bool,
    ) {
        let job_id = st
            .evals
            .iter()
            .find_map(|view| {
                view.jobs.iter().find(|job| job.claim.as_ref().is_some_and(|c| c.claim_id == claim)).map(|job| job.job.id())
            })
            .unwrap_or_default();
        let (kind, detail) = match &found {
            Some(Ok(PalwEvalDisputeV1::Agrees)) => {
                ("unlocated", format!("the served capture agrees with the honest run, yet the replay differs: {why}"))
            }
            Some(Ok(located)) => (located.kind(), located.describe()),
            Some(Err(e)) => ("unlocated", format!("{why}; the accused's capture cannot be used: {e}")),
            None => ("unlocated", why.to_string()),
        };
        let filing = match (filing, files) {
            (Some(Ok(f)), _) => format!(
                "accusation built ({} close{}): waits for the court's carrier",
                f.label,
                if f.opens_dissection { ", the named leaf is dissected: it opens a dissection" } else { "" }
            ),
            (Some(Err(e)), _) => format!("not filed: {e}"),
            (None, false) => {
                "not filed: this node files no accusation (it is not a challenger, or has no bond and key to accuse under)".to_string()
            }
            (None, true) => {
                "not filed: the dispute is not located (the accused's capture is not served, or it agrees with the honest run)"
                    .to_string()
            }
        };
        warn!("[{PALW_PANEL}] [palw-improve] DISPUTE evaluation claim {claim}: {kind}: {detail}; {filing}");
        let found_daa = st.daa;
        st.disputes.insert(claim, PalwImproveDisputeV1 { claim_id: claim, job_id, kind, detail, found_daa, filing });
    }

    /// **Say how a dispute stands now** (the status file's `filing`).
    fn improve_set_filing_v1(st: &mut PalwImproveLoopV1, claim: Hash64, filing: String) {
        if let Some(dispute) = st.disputes.get_mut(&claim) {
            dispute.filing = filing;
        }
    }

    /// **The evaluation court's pass** (RFC-0004 A10, spec 17 §17.8.6): sign and queue the accusations a challenger's audit
    /// built. An evaluation accusation is the IR one-move accusation it rides as — `TirShardCourtAccused` (tag 62) over the
    /// claim's roots and executor bond, the accuser this node's bond, the verdict `ExecutorGuilty` the proof was checked to
    /// support, the accuser's ML-DSA-87 over `palw_tir_one_move_session_id_v1` under the accusation context — and takes the
    /// court's own carrier path (`court_pending`, `court_due`: due now, like every one-move accusation, so the priority
    /// lane carries it ahead of undated items and it folds while the claim can still be convicted). The claim is read from
    /// the chain's evaluation view as of now: one already void or `Final` is not accused (the accusation would be refused
    /// whole), and a claim another challenger's accusation convicted first is simply gone.
    pub(super) fn improve_court_pass_v1(
        &self,
        st: &mut PalwImproveLoopV1,
        bond_key: PalwBondKeyV2,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        if st.court_ready.is_empty() {
            return;
        }
        for (claim, filing) in std::mem::take(&mut st.court_ready) {
            if st.court_done.contains(&claim) {
                continue;
            }
            let view =
                st.evals.iter().flat_map(|v| v.jobs.iter()).find_map(|j| j.claim.as_ref().filter(|c| c.claim_id == claim)).cloned();
            let Some(view) = view.filter(|c| !c.voided && c.final_daa.is_none()) else {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: it is void, final or gone from the chain's evaluation \
                     view: the accusation built for it is not filed"
                );
                Self::improve_set_filing_v1(
                    st,
                    claim,
                    "not filed: the claim was void, final or gone from the chain's evaluation view before the accusation could ride"
                        .to_string(),
                );
                st.court_done.insert(claim);
                continue;
            };
            let mut accusation = PalwTirOneMoveAccusationV1 {
                version: PALW_TIR_ONE_MOVE_VERSION_V1,
                claim,
                execution_root: view.execution_root,
                trace_root: view.trace_root,
                executor_bond: view.bond,
                accuser_bond: bond_key,
                verdict: PalwCourtVerdictV2::ExecutorGuilty,
                proof: filing.proof.clone(),
                signature: Vec::new(),
            };
            let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.consensus_config.params.net.to_string().as_bytes(),
                Some(self.consensus_config.genesis.hash),
            );
            let session_id = palw_tir_one_move_session_id_v1(domain.as_byte_slice(), &accusation);
            let Some(signature) = self.sign(session_id.as_byte_slice(), PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1) else {
                // No key at this moment: the accusation waits for the next pass.
                st.court_ready.push((claim, filing));
                continue;
            };
            accusation.signature = signature;
            if let Err(why) = palw_tir_one_move_shape_v1(&accusation) {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: its accusation is malformed ({why}); recorded, not filed"
                );
                Self::improve_set_filing_v1(st, claim, format!("not filed: the accusation is malformed ({why})"));
                st.court_done.insert(claim);
                continue;
            }
            let object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) };
            if let Err(why) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: the accusation cannot ride a carrier ({why}); recorded, not filed"
                );
                Self::improve_set_filing_v1(st, claim, format!("not filed: the accusation cannot ride a carrier ({why})"));
                st.court_done.insert(claim);
                continue;
            }
            st.court_done.insert(claim);
            if court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) {
                continue;
            }
            info!(
                "[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: filing its one-move accusation ({} close, leaf {:?}{}), session {session_id}",
                filing.label,
                filing.leaf,
                if filing.opens_dissection { ", a dissected leaf: the chain opens a dissection there" } else { "" }
            );
            court_due.insert((session_id, 0, false), current_daa);
            court_pending.push((session_id, 0, false, object));
            Self::improve_set_filing_v1(
                st,
                claim,
                format!("filed: TirShardCourtAccused ({} close) as session {session_id} at DAA {current_daa}", filing.label),
            );
        }
    }
}

/// Why an audit did not start.
enum PalwImproveAuditWaitV1 {
    /// The chain's view does not hold the claim (or its item's material) yet: asked again as the view moves.
    ChainView,
    /// This node does not hold the claim's subject class.
    NotHeld,
}

/// The most step leaves a node retains a capture for (a capture is every leaf's lanes; a larger claim's
/// evidence is served from a re-run).
const PALW_IMPROVE_CAPTURE_MAX_LEAVES_V1: u64 = 1 << 20;

#[cfg(test)]
mod tests {
    /// **The panel hooks the improvement loop in four places and nowhere else**: the tick after the
    /// chain reads, the carrier at the `Own` site beside the canonical claim, the seat's step at the
    /// head of the verdict block, and the evaluation court's pass beside the IR one-move pass — each behind
    /// the fence (the tick) or the claim's own predicate (the seat), none changing a line below it.
    #[test]
    fn the_panel_loop_hooks_the_improvement_loop_in_its_four_places() {
        let panel = include_str!("../palw_panel.rs");
        assert!(panel.contains("self.improve_tick_v1(&mut improve, current_daa).await"));
        let one_move = panel.find("self.tir_one_move_pass_v1(").expect("the IR one-move pass");
        let court = panel
            .find("self.improve_court_pass_v1(&mut improve, bond_key, current_daa, &mut court_pending, &mut court_due)")
            .expect("the evaluation court's pass");
        assert!(one_move < court, "the evaluation accusations are queued after the IR pass, on the same court path");
        assert!(
            court
                < panel[one_move..]
                    .find("// --- the court's half: answer the disputes this bond is a party to ---")
                    .expect("the court's half")
                    + one_move,
            "and before the court's half answers its duties"
        );
        assert!(
            panel
                .contains(".improve_carry_v1(&mut improve, &session, network_domain, bond, current_daa, &mut funding, &mut inflight)")
        );
        let verdict = &panel[panel.find("let verdict = 'verdict: {").expect("the verdict block")..];
        let hook = verdict.find("self.improve_seat_step_v1(&mut improve, duty).await").expect("the seat's hook");
        let capable =
            verdict.find("self.resolve_backend(&session, duty.class_id, duty.artifact_root).is_err()").expect("the capability check");
        assert!(capable < hook, "a seat that does not hold the class is Incapable before it is asked to replay");
        assert!(
            hook < verdict.find("if duty.free_prompt {").expect("the free-prompt arms"),
            "the evaluation step precedes the FP arms"
        );
    }
}
