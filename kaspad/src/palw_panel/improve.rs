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
//! **Dormant**: nothing here runs below the fence.

use super::*;

use std::collections::VecDeque;
use std::path::Path;

use kaspa_consensus_core::palw_improve_node_v1::{PalwImprovementEvalViewV1, PalwImprovementStatusV1};
use kaspa_consensus_core::palw_improve_state_v1::PalwImprovementEpochViewV1;
use misaka_palw_sdk::improve::{
    PalwImproveDutyV1, PalwImproveEvalTaskV1, PalwImproveNodeChainV1, PalwImproveNodeV1, PalwImprovePrefetchV1,
    palw_improve_stage_params_of_v1,
};
use misaka_palw_sdk::improve_eval::{
    PalwEvalCaptureV1, PalwEvalClaimFactsV1, PalwEvalCommittedRootsV1, PalwEvalHeldV1, PalwEvalSeatJudgmentV1, PalwEvalWorkV1,
    palw_eval_claim_v1, palw_eval_run_v1, palw_eval_seat_judge_roots_v1,
};

use crate::palw_improve_watch::{
    PALW_IMPROVE_READ_EVERY_V1, PalwImproveWatchV1, palw_improve_admitting_classes_v1, palw_improve_held_classes_v1,
    palw_improve_log_tick_v1, palw_improve_status_json_v1, palw_improve_watch_armed_v1,
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
    /// Seat replays of evaluation claims, in flight and done.
    seat_runs: HashMap<Hash64, tokio::task::JoinHandle<PalwEvalSeatJudgmentV1>>,
    seat_done: HashMap<Hash64, PalwEvalSeatJudgmentV1>,
    /// The section files this node already tried against a candidate (by path and modification time).
    sections_tried: HashSet<(PathBuf, u64)>,
    /// Jobs this node ran whose claim came out past the job's share of the epoch's position budget
    /// (MIP-20): the chain would refuse it whole, so it is neither carried nor run again.
    over_budget: HashSet<Hash64>,
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
        self.improve_reap_v1(st, current_daa).await;
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
        self.improve_write_status_v1(&status, current_daa);
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
                    st.attempted.insert(task.job_id, current_daa);
                    let run = task.clone();
                    st.running.push(PalwImproveRunV1 {
                        task,
                        handle: tokio::task::spawn_blocking(move || palw_eval_run_v1(&held, &run)),
                    });
                }
            }
        }
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
    fn improve_write_status_v1(&self, status: &PalwImprovementStatusV1, daa: u64) {
        let path = self.config.state_dir.join("palw-improve-status.json");
        let json = palw_improve_status_json_v1(status, daa);
        let _ = std::fs::create_dir_all(&self.config.state_dir);
        let partial = path.with_extension("json.partial");
        if let Err(e) = std::fs::write(&partial, serde_json::to_vec_pretty(&json).unwrap_or_default()).and_then(|()| std::fs::rename(&partial, &path)) {
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
    fn improve_prefetch_v1(&self, st: &mut PalwImproveLoopV1, line_id: Hash64, epoch: u64, class_id: Hash64, plan: PalwImprovePrefetchV1) {
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
            st.sections_tried.insert(key);
            // The parent this node holds, from whichever loader mapped it (the process-wide cache hands one
            // out): the section opens over that entry, not over a lineage's own list.
            let PalwImprovePrefetchV1::Adapter { parent_class, parent_root, .. } = plan else {
                warn!("[{PALW_PANEL}] [palw-improve] full-weight prefetch of {} is not served by this build", path.display());
                return;
            };
            let holdings = self.backends().holdings().to_vec();
            let Some(parent) = misaka_palw_sdk::tir_registration::tir_entries_of_v1(&holdings)
                .into_iter()
                .find(|e| e.artifact.composite_ref().is_none() && e.class_id() == parent_class && e.artifact_root == parent_root)
            else {
                warn!("[{PALW_PANEL}] [palw-improve] the parent {parent_class} of candidate {class_id} is not held: nothing to open {} over", path.display());
                return;
            };
            match misaka_palw_sdk::lineages::tir::TirLineageV1::load_composite_over(&parent, &path) {
                Ok(loaded) => {
                    let declared: Vec<Hash64> =
                        misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&loaded)).iter().map(|e| e.class_id()).collect();
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
                Err(why) => warn!("[{PALW_PANEL}] [palw-improve] {} is not the candidate {class_id}'s artifact: {why}", path.display()),
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
            key.build_fp_eval_commitment_tx(claim.commitment.clone(), &claim.tail, claim.prompt.clone(), funding_outpoint, funding, fee)
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
        // A claim another node's took meanwhile is not carried: the first valid claim per job is the one.
        let taken = {
            let chain = PalwImproveNodeChainV1 { views: &st.views, eval: &st.evals };
            misaka_palw_sdk::improve::PalwImproveChainV1::job_claimed(&chain, &work.task.job_id)
        };
        if taken {
            info!("[{PALW_PANEL}] [palw-improve] job {} is taken already: not carrying this node's claim", work.task.job_id);
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
                return false;
            }
        }
        let (tx, claim_id) = match self.improve_build_claim_tx_v1(st, session, network_domain, bond, current_daa, &work, funding_outpoint, &funding_entry) {
            Ok(built) => built,
            Err(why) => {
                warn!(
                    "[{PALW_PANEL}] [palw-improve] cannot build the claim for item {} of epoch {} ({:?}): {why}",
                    work.task.item, work.task.epoch, work.task.subject
                );
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
                    let dir = self.config.retention_dir.join("improve");
                    let file = dir.join(format!("{claim_id}.capture"));
                    let partial = dir.join(format!("{claim_id}.capture.partial"));
                    if std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&partial, &bytes)).and_then(|()| std::fs::rename(&partial, &file)).is_err() {
                        warn!("[{PALW_PANEL}] [palw-improve] cannot retain the capture of claim {claim_id}");
                    }
                }
                let next = TransactionOutpoint::new(txid, 0);
                self.persist_fee_outpoint(next);
                *funding = Some((
                    next,
                    UtxoEntry { amount: change.value, script_public_key: change.script_public_key, block_daa_score: current_daa, is_coinbase: false },
                ));
                *inflight += 1;
                true
            }
            Err(e) => {
                warn!("[{PALW_PANEL}] [palw-improve] the mempool refused the evaluation claim: {e}");
                // The job is offered again at the retry age; the funding outpoint may be stale.
                *funding = None;
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
        let epoch = st.views.iter().find(|e| e.line.line_id == view.line_id && e.epoch.epoch == view.epoch).ok_or("the epoch is not open")?;
        let item_view = view.items.iter().find(|i| i.item == job.job.item).ok_or("the item's material is not on chain")?;
        let params = palw_improve_stage_params_of_v1(&epoch.policy, job.job.kind).ok_or("the policy has no stage of the job's kind")?;
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
        if let Some(handle) = st.seat_runs.get(&claim) {
            if !handle.is_finished() {
                return PalwImproveSeatStepV1::Waiting;
            }
            let handle = st.seat_runs.remove(&claim).expect("the handle just read");
            let judgment = handle.await.unwrap_or_else(|e| PalwEvalSeatJudgmentV1::Unjudgeable(format!("the replay did not finish: {e}")));
            let step = palw_improve_seat_step_of_v1(&judgment);
            match &judgment {
                PalwEvalSeatJudgmentV1::Valid => info!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: the replay reproduces it"),
                PalwEvalSeatJudgmentV1::Differs(why) => {
                    warn!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: the replay differs — the court's question: {why}")
                }
                PalwEvalSeatJudgmentV1::Unjudgeable(why) => {
                    warn!("[{PALW_PANEL}] [palw-improve] evaluation claim {claim}: cannot be judged: {why}")
                }
            }
            st.seat_done.insert(claim, judgment);
            return step;
        }
        let (task, roots) = match Self::improve_task_of_claim_v1(st, &claim) {
            Ok(derived) => derived,
            // The view moves with the chain: wait for it (it is read every few seconds), never answer.
            Err(why) => {
                crate::palw_backends::note_throttled_v1("improve-seat-waits", || {
                    format!("[{PALW_PANEL}] [palw-improve] claim {claim}: not judged yet — {why}")
                });
                return PalwImproveSeatStepV1::Waiting;
            }
        };
        let Some(held) = st.held.get(&task.subject_class).cloned() else {
            crate::palw_backends::note_throttled_v1("improve-seat-unheld", || {
                format!("[{PALW_PANEL}] [palw-improve] claim {claim}: this seat does not hold class {}", task.subject_class)
            });
            return PalwImproveSeatStepV1::Unjudgeable;
        };
        info!(
            "[{PALW_PANEL}] [palw-improve] replaying evaluation claim {claim}: item {} of epoch {} for {:?} ({:?})",
            task.item, task.epoch, task.subject, task.kind
        );
        st.seat_runs.insert(claim, tokio::task::spawn_blocking(move || palw_eval_seat_judge_roots_v1(&held, &task, &roots)));
        PalwImproveSeatStepV1::Waiting
    }
}

/// The most step leaves a node retains a capture for (a capture is every leaf's lanes; a larger claim's
/// evidence is served from a re-run).
const PALW_IMPROVE_CAPTURE_MAX_LEAVES_V1: u64 = 1 << 20;

#[cfg(test)]
mod tests {
    /// **The panel hooks the improvement loop in three places and nowhere else**: the tick after the
    /// chain reads, the carrier at the `Own` site beside the canonical claim, and the seat's step at the
    /// head of the verdict block — each behind the fence (the tick) or the claim's own predicate (the
    /// seat), none changing a line below it.
    #[test]
    fn the_panel_loop_hooks_the_improvement_loop_in_its_three_places() {
        let panel = include_str!("../palw_panel.rs");
        assert!(panel.contains("self.improve_tick_v1(&mut improve, current_daa).await"));
        assert!(panel.contains(".improve_carry_v1(&mut improve, &session, network_domain, bond, current_daa, &mut funding, &mut inflight)"));
        let verdict = &panel[panel.find("let verdict = 'verdict: {").expect("the verdict block")..];
        let hook = verdict.find("self.improve_seat_step_v1(&mut improve, duty).await").expect("the seat's hook");
        let capable = verdict.find("self.resolve_backend(&session, duty.class_id, duty.artifact_root).is_err()").expect("the capability check");
        assert!(capable < hook, "a seat that does not hold the class is Incapable before it is asked to replay");
        assert!(hook < verdict.find("if duty.free_prompt {").expect("the free-prompt arms"), "the evaluation step precedes the FP arms");
    }
}
