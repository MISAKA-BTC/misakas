//! **RFC-0004's node side (work item A10): what a node does for a governed line's epochs.**
//!
//! The chain decides everything an epoch does (`docs/rfc/0004-palw-model-improvement.md` §4): its
//! transitions happen at DAA boundaries, its items are drawn by R, its scores are the committed outputs
//! of evaluation claims, and the fold promotes. A node's part is to *serve* that machine:
//!
//! * **watch** every governed line's epoch — [`palw_improve_duties_v1`] reads the chain through
//!   [`PalwImproveChainV1`] and says what this node should do now;
//! * **prefetch** each candidate it can hold — a composite candidate over a parent it holds costs
//!   only the adapter section (§6.7); full weights only where the policy admits them and the node
//!   opted in ([`palw_improve_prefetch_plan_v1`]);
//! * **evaluate** — every `(item, subject)` job of an `Evaluating` epoch whose subject class it
//!   holds and that no claim has taken, derived exactly as the chain derives it
//!   ([`palw_improve_eval_job_v1`]: the item's seed is the epoch's, the job id the chain's formula),
//!   and run by the evaluation executor ([`crate::improve_eval`]).
//!
//! **Against interfaces, not the row layout.** The core lane's rows may move (items and results into
//! keyed tables); everything here reads the chain through [`PalwImproveChainV1`] and its view types,
//! and [`PalwImproveRowsV1`] is the one place that reads step 0's rows.
//!
//! **Provisional until A6** (the evaluation job family): which scoring stage a case's pipeline ends
//! in ([`palw_improve_scoring_kind_of_v1`]) and the pipeline root the job names. They are read off
//! the policy's `eval.stages` here, and A6's pipeline registry replaces the lookup.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_material_v1::PalwCaseReferenceV1;
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwEpochCandidateV1, PalwEpochStateV1, PalwEpochTimesV1, PalwEvalItemV1, PalwEvalJobV1, PalwEvalModeV1, PalwEvalSubjectV1,
    PalwImprovementEpochV1, PalwImprovementLineV1, PalwImprovementPolicyV1, PalwScoringKindV1, palw_improve_eval_job_id_v1,
    palw_improve_eval_seed_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;

// ---------------------------------------------------------------------------------------------
// The chain, as a node reads it
// ---------------------------------------------------------------------------------------------

/// **A governed line, as the watcher reads it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImproveLineViewV1 {
    pub line_id: Hash64,
    pub owner: PalwBondKeyV2,
    /// The line's head: its current version's IR class id (every candidate's parent in an open epoch).
    pub head: Hash64,
    pub policy: PalwImprovementPolicyV1,
    /// The epoch the line runs now, if any (one at a time, PALW-MIP-5).
    pub open_epoch: Option<u64>,
}

/// **An epoch, as the watcher reads it** — its state and clock, its parent and candidates, its seed.
/// Its items are read by key ([`PalwImproveChainV1::items`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImproveEpochViewV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub state: PalwEpochStateV1,
    pub times: PalwEpochTimesV1,
    /// The head when the epoch opened: every candidate's parent, and the `Parent` subject's class.
    pub parent: Hash64,
    /// In acceptance order (the order ties go by, §7.5).
    pub candidates: Vec<PalwEpochCandidateV1>,
    /// The epoch seed, from `Drawn` on.
    pub seed: Option<Hash64>,
}

/// **What an item evaluates** — its prompt and its reference (a key's or a continuation's commitment,
/// or none for a judged case), whatever its source (a hold-out hard case, a setter's revealed prompt,
/// a suite's item).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvalCaseViewV1 {
    pub prompt_ids: Vec<u32>,
    pub reference: PalwCaseReferenceV1,
    pub domain: u16,
}

/// **What the node reads of the chain** — the only door the watcher and the executor take into the
/// improvement tables. A kaspad adapter implements it over the tip's state; tests over
/// [`PalwImproveRowsV1`].
pub trait PalwImproveChainV1 {
    /// Every governed line, by id.
    fn governed_lines(&self) -> Vec<Hash64>;
    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1>;
    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1>;
    /// The epoch's drawn items, in item order (empty before `Drawn`).
    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1>;
    /// The case item `item` of the epoch evaluates, when the chain holds it (a setter's prompt only
    /// once revealed).
    fn case(&self, line_id: &Hash64, epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1>;
    /// Whether evaluation job `job_id` is taken: the first valid claim per job is the one (§7.2).
    fn job_claimed(&self, job_id: &Hash64) -> bool;
}

/// **Step 0's rows as a chain view** — the one place that reads the rows' layout
/// (`palw_improve_state_v1`): the line and epoch rows, the cases by id, the claimed jobs.
#[derive(Clone, Debug, Default)]
pub struct PalwImproveRowsV1 {
    pub lines: BTreeMap<Hash64, PalwImprovementLineV1>,
    pub epochs: BTreeMap<(Hash64, u64), PalwImprovementEpochV1>,
    pub cases: BTreeMap<Hash64, PalwEvalCaseViewV1>,
    pub claimed: BTreeSet<Hash64>,
}

impl PalwImproveChainV1 for PalwImproveRowsV1 {
    fn governed_lines(&self) -> Vec<Hash64> {
        self.lines.keys().copied().collect()
    }

    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1> {
        let row = self.lines.get(line_id)?;
        Some(PalwImproveLineViewV1 {
            line_id: row.line_id,
            owner: row.owner,
            head: row.head,
            policy: row.policy.clone(),
            open_epoch: row.open_epoch,
        })
    }

    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1> {
        let row = self.epochs.get(&(*line_id, epoch))?;
        Some(PalwImproveEpochViewV1 {
            line_id: row.line_id,
            epoch: row.epoch,
            state: row.state,
            times: row.times,
            parent: row.parent,
            candidates: row.candidates.clone(),
            seed: row.seed,
        })
    }

    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1> {
        self.epochs.get(&(*line_id, epoch)).map(|row| row.items.clone()).unwrap_or_default()
    }

    fn case(&self, _line_id: &Hash64, _epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1> {
        self.cases.get(&item.case_id).cloned()
    }

    fn job_claimed(&self, job_id: &Hash64) -> bool {
        self.claimed.contains(job_id)
    }
}

// ---------------------------------------------------------------------------------------------
// The node
// ---------------------------------------------------------------------------------------------

/// **What this node can do for the protocol.**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwImproveNodeV1 {
    /// The IR classes this node holds and can run (a line's head, prefetched candidates).
    pub holds: BTreeSet<Hash64>,
    /// It runs evaluation jobs (an executor with claim capacity).
    pub evaluates: bool,
    /// It fetches full-weight candidates too (their prefetch is the whole artifact, §6.7, §13).
    pub prefetch_full: bool,
}

// ---------------------------------------------------------------------------------------------
// Prefetch (RFC-0004 §6.7)
// ---------------------------------------------------------------------------------------------

/// **What fetching a candidate's artifact costs this node.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwImprovePrefetchV1 {
    /// A composite candidate over a parent this node holds: the adapter section only — params `p..`
    /// under `adapter_root`; params `0..p` are the parent's, served from the parent's inventory.
    Adapter { parent_class: Hash64, parent_root: Hash64, adapter_root: Hash64, p: u32 },
    /// Full weights: the whole artifact under `root`.
    Full { root: Hash64 },
}

/// **How this node would fetch `candidate`**, or why it does not: a composite candidate needs its
/// parent held (a node without the parent is not the candidate's seat); full weights need the policy
/// to admit them (the chain refuses them otherwise) and this node to have opted in.
pub fn palw_improve_prefetch_plan_v1(
    candidate: &PalwEpochCandidateV1,
    node: &PalwImproveNodeV1,
    policy: &PalwImprovementPolicyV1,
) -> Result<PalwImprovePrefetchV1, &'static str> {
    match candidate.artifact {
        PalwTirArtifactRefV1::Composite { parent_class, parent_root, adapter_root, p } => {
            if node.holds.contains(&parent_class) {
                Ok(PalwImprovePrefetchV1::Adapter { parent_class, parent_root, adapter_root, p })
            } else {
                Err("a composite candidate over a parent this node does not hold")
            }
        }
        PalwTirArtifactRefV1::Single { root } => {
            if !policy.provenance.full_weight_candidates {
                Err("the line's policy admits no full-weight candidate")
            } else if !node.prefetch_full {
                Err("this node does not prefetch full-weight candidates")
            } else {
                Ok(PalwImprovePrefetchV1::Full { root })
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Evaluation jobs (RFC-0004 §7.2)
// ---------------------------------------------------------------------------------------------

/// **The subjects of an epoch's items**, in the chain's order: the parent, then each candidate in
/// acceptance order — with the class each runs.
pub fn palw_improve_subjects_v1(epoch: &PalwImproveEpochViewV1) -> Vec<(PalwEvalSubjectV1, Hash64)> {
    std::iter::once((PalwEvalSubjectV1::Parent, epoch.parent))
        .chain(epoch.candidates.iter().map(|c| (PalwEvalSubjectV1::Candidate(c.class_id), c.class_id)))
        .collect()
}

/// **The scoring stage a case is scored by** (provisional until A6's pipeline registry): an exact key
/// by `ExactMatch`, a continuation by `RefLogLik`, a case with no reference by a `Judge`.
pub fn palw_improve_scoring_kind_of_v1(reference: &PalwCaseReferenceV1) -> PalwScoringKindV1 {
    match reference {
        PalwCaseReferenceV1::ExactKey { .. } => PalwScoringKindV1::ExactMatch,
        PalwCaseReferenceV1::Continuation { .. } => PalwScoringKindV1::RefLogLik,
        PalwCaseReferenceV1::None => PalwScoringKindV1::Judge,
    }
}

/// **The evaluation job of `item` for `subject`, derived as the chain derives it** — with its id
/// (`H(line ‖ epoch ‖ item ‖ subject)`). The item's seed must be the epoch's
/// (`H(epoch seed ‖ item)`, the same for every subject, so pairing is exact); the mode follows the
/// case (a continuation is teacher-forced, anything else generated under the item's seed with the
/// policy's budget and stop ids); the pipeline is the policy's stage of the case's scoring kind.
pub fn palw_improve_eval_job_v1(
    line: &PalwImproveLineViewV1,
    epoch: &PalwImproveEpochViewV1,
    item: &PalwEvalItemV1,
    subject: PalwEvalSubjectV1,
    case: &PalwEvalCaseViewV1,
) -> Result<(PalwEvalJobV1, Hash64), &'static str> {
    let epoch_seed = epoch.seed.ok_or("the epoch has no seed yet: its items are drawn at Drawn")?;
    if item.seed != palw_improve_eval_seed_v1(&epoch_seed, item.item) {
        return Err("the item's seed is not the epoch's");
    }
    let eval = &line.policy.eval;
    let mode = match case.reference {
        PalwCaseReferenceV1::Continuation { commitment } => PalwEvalModeV1::TeacherForced { reference_commitment: commitment },
        PalwCaseReferenceV1::ExactKey { .. } | PalwCaseReferenceV1::None => {
            PalwEvalModeV1::Generate { seed: item.seed, max_new: eval.max_new_tokens, stop_ids: eval.stop_ids.clone() }
        }
    };
    let kind = palw_improve_scoring_kind_of_v1(&case.reference);
    let pipeline_root = eval
        .stages
        .iter()
        .find(|stage| stage.kind == kind)
        .map(|stage| stage.program_root)
        .ok_or("the policy's eval spec has no stage of the case's scoring kind")?;
    let job = PalwEvalJobV1 { line_id: line.line_id, epoch: epoch.epoch, item: item.item, subject, mode, pipeline_root };
    let job_id = palw_improve_eval_job_id_v1(&line.line_id, epoch.epoch, item.item, &subject);
    Ok((job, job_id))
}

// ---------------------------------------------------------------------------------------------
// The watcher's plan
// ---------------------------------------------------------------------------------------------

/// **Something this node should do for a governed line now.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwImproveDutyV1 {
    /// Fetch a candidate's artifact (or its adapter section) before the epoch evaluates it.
    Prefetch { line_id: Hash64, epoch: u64, class_id: Hash64, plan: PalwImprovePrefetchV1 },
    /// Run an evaluation job and claim it before `until_daa` (the epoch's `t_eval`).
    Evaluate { job: PalwEvalJobV1, job_id: Hash64, until_daa: u64 },
}

/// Do an epoch's candidates want prefetching in `state`? From the moment they are known (they are
/// submitted in `Submission`) until evaluation ends.
fn prefetches_in(state: PalwEpochStateV1) -> bool {
    matches!(state, PalwEpochStateV1::Submission | PalwEpochStateV1::HoldOut | PalwEpochStateV1::Drawn | PalwEpochStateV1::Evaluating)
}

/// **What this node should do now for every governed line** — pure, over the chain view:
///
/// * every candidate of an epoch between `Submission` and `Evaluating` that this node does not hold
///   and can fetch ([`palw_improve_prefetch_plan_v1`]);
/// * for an evaluating node, every job of an `Evaluating` epoch before its `t_eval` — each item for
///   each subject whose class this node holds — that no claim has taken, derived as the chain derives
///   it, in item order then subject order, at most `max_jobs`. An item whose case the chain does not
///   hold yet (a setter's prompt before its reveal) or that derives no job is skipped.
pub fn palw_improve_duties_v1(
    chain: &dyn PalwImproveChainV1,
    node: &PalwImproveNodeV1,
    current_daa: u64,
    max_jobs: usize,
) -> Vec<PalwImproveDutyV1> {
    let mut duties = Vec::new();
    let mut jobs = 0usize;
    for line_id in chain.governed_lines() {
        let Some(line) = chain.line(&line_id) else { continue };
        let Some(epoch) = line.open_epoch.and_then(|e| chain.epoch(&line_id, e)) else { continue };
        if prefetches_in(epoch.state) {
            for candidate in &epoch.candidates {
                if node.holds.contains(&candidate.class_id) {
                    continue;
                }
                if let Ok(plan) = palw_improve_prefetch_plan_v1(candidate, node, &line.policy) {
                    duties.push(PalwImproveDutyV1::Prefetch { line_id, epoch: epoch.epoch, class_id: candidate.class_id, plan });
                }
            }
        }
        if !node.evaluates || epoch.state != PalwEpochStateV1::Evaluating || current_daa >= epoch.times.t_eval {
            continue;
        }
        let subjects: Vec<(PalwEvalSubjectV1, Hash64)> =
            palw_improve_subjects_v1(&epoch).into_iter().filter(|(_, class)| node.holds.contains(class)).collect();
        for item in chain.items(&line_id, epoch.epoch) {
            let Some(case) = chain.case(&line_id, epoch.epoch, &item) else { continue };
            for (subject, _) in &subjects {
                if jobs >= max_jobs {
                    return duties;
                }
                let Ok((job, job_id)) = palw_improve_eval_job_v1(&line, &epoch, &item, *subject, &case) else { continue };
                if chain.job_claimed(&job_id) {
                    continue;
                }
                duties.push(PalwImproveDutyV1::Evaluate { job, job_id, until_daa: epoch.times.t_eval });
                jobs += 1;
            }
        }
    }
    duties
}

/// **What changed in a line's epoch since the node last looked** — for the watcher's log: the epoch
/// and the state it entered, or `None` when nothing moved.
pub fn palw_improve_epoch_moved_v1(
    seen: Option<(u64, PalwEpochStateV1)>,
    now: Option<(u64, PalwEpochStateV1)>,
) -> Option<(u64, PalwEpochStateV1)> {
    match (seen, now) {
        (_, None) => None,
        (Some(before), Some(after)) if before == after => None,
        (_, Some(after)) => Some(after),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_state_v1::{
        PALW_IMPROVEMENT_POLICY_VERSION_V1, PalwEpochWindowsV1, PalwEvalSpecV1, PalwImprovementFeesV1, PalwImprovementPoolV1,
        PalwItemSourceV1, PalwProvenancePolicyV1, PalwScoringStageV1, PalwUsageMeasureV1, PalwUsageThresholdV1,
        palw_improvement_policy_digest_v1,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    pub fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    pub fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }

    pub fn policy(full_weights: bool) -> PalwImprovementPolicyV1 {
        PalwImprovementPolicyV1 {
            version: PALW_IMPROVEMENT_POLICY_VERSION_V1,
            usage: PalwUsageThresholdV1 { measure: PalwUsageMeasureV1::Claims, value: 10 },
            windows: PalwEpochWindowsV1 {
                grid: 100,
                w_collect: 50,
                w_submit: 50,
                w_holdout: 20,
                w_eval: 60,
                beacon_delay: 2,
                court_margin: 10,
            },
            eval: PalwEvalSpecV1 {
                stages: vec![
                    PalwScoringStageV1 { kind: PalwScoringKindV1::ExactMatch, program_root: h(0xE1) },
                    PalwScoringStageV1 { kind: PalwScoringKindV1::RefLogLik, program_root: h(0xE2) },
                    PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, program_root: h(0xE3) },
                ],
                regression_suite_root: h(0),
                regression_items: 0,
                safety_suite_root: h(0),
                safety_items: 0,
                judge_set: vec![h(0x7D)],
                anchor_floor_permille: 800,
                n: 8,
                n_min: 4,
                delta_permille: 50,
                epsilon_permille: 20,
                epsilon_safety_permille: 0,
                alpha_permille: 50,
                max_new_tokens: 32,
                stop_ids: vec![2],
                setter_cap_permille: 300,
            },
            k_max: 4,
            fees: PalwImprovementFeesV1 {
                registration_fee: 1,
                candidate_bond: 1,
                eval_fee_per_job: 1,
                hard_case_fee: 1,
                artifact_bond: 1,
                setter_bond: 1,
                dataset_bond: 1,
            },
            phi_permille: 100,
            bounty_share_permille: 100,
            s2_trainer_permille: 500,
            s2_dataset_cap_permille: 200,
            s2_contributor_cap_permille: 100,
            provenance: PalwProvenancePolicyV1 {
                teacher_classes: 0xFF,
                licence_classes: vec![],
                full_weight_candidates: full_weights,
                base_licence_class: h(0),
            },
            rollback_epochs: 2,
            vest_epochs: 2,
            ban_epochs: 2,
        }
    }

    pub const HEAD: u64 = 0x4EAD;
    pub const LINE: u64 = 0x11E;

    /// A governed line whose epoch 3 is in `state`, with a composite candidate over the head and a
    /// full-weight one, and three drawn items: an exact key, a continuation, a judged case.
    pub fn rows(state: PalwEpochStateV1, full_weights: bool) -> PalwImproveRowsV1 {
        let policy = policy(full_weights);
        let seed = h(0x5EED);
        let composite = PalwTirArtifactRefV1::Composite { parent_class: h(HEAD), parent_root: h(0x400), adapter_root: h(0xAD), p: 40 };
        let candidate = |class: u64, artifact: PalwTirArtifactRefV1, n: u64| PalwEpochCandidateV1 {
            class_id: h(class),
            submitter: bond(n),
            artifact,
            declarations_digest: h(0),
            fees_paid: 1,
            bond: 1,
            submitted_daa: 160 + n,
        };
        let item = |i: u32, case: u64| PalwEvalItemV1 {
            item: i,
            case_id: h(case),
            source: PalwItemSourceV1::HoldOut,
            seed: palw_improve_eval_seed_v1(&seed, i),
            judge: None,
        };
        let line = PalwImprovementLineV1 {
            line_id: h(LINE),
            owner: bond(1),
            policy_digest: palw_improvement_policy_digest_v1(&policy),
            policy,
            governed_from_daa: 0,
            pending_policy: None,
            opt_out_after_epoch: None,
            head: h(HEAD),
            head_history: vec![],
            usage_baseline: 0,
            next_epoch: 4,
            open_epoch: Some(3),
            pool: PalwImprovementPoolV1 { balance: 0, deposited: 0, paid: 0, forfeited_in: 0, refunded: 0 },
            barred_submitters: vec![],
        };
        let epoch = PalwImprovementEpochV1 {
            line_id: h(LINE),
            epoch: 3,
            state,
            times: PalwEpochTimesV1 { t_open: 100, t_fix: 150, t_close: 200, t_draw: 220, t_eval: 280 },
            parent: h(HEAD),
            material_acc: h(0),
            material_count: 0,
            dataset_root: None,
            holdout_cases: vec![],
            setter_sets: vec![],
            candidates: vec![candidate(0xC1, composite, 2), candidate(0xC2, PalwTirArtifactRefV1::Single { root: h(0xF0) }, 3)],
            seed: Some(seed),
            items: vec![item(0, 0xCA0), item(1, 0xCA1), item(2, 0xCA2)],
            results: vec![],
            counts: vec![],
            outcome: None,
            grants: vec![],
        };
        let case = |reference: PalwCaseReferenceV1| PalwEvalCaseViewV1 { prompt_ids: vec![1, 5, 9], reference, domain: 0 };
        PalwImproveRowsV1 {
            lines: [(h(LINE), line)].into(),
            epochs: [((h(LINE), 3), epoch)].into(),
            cases: [
                (h(0xCA0), case(PalwCaseReferenceV1::ExactKey { commitment: h(0xB0) })),
                (h(0xCA1), case(PalwCaseReferenceV1::Continuation { commitment: h(0xB1) })),
                (h(0xCA2), case(PalwCaseReferenceV1::None)),
            ]
            .into(),
            claimed: BTreeSet::new(),
        }
    }

    /// **Prefetch follows the artifact**: a composite candidate over a held parent is its adapter
    /// section; over a parent not held, nothing; full weights only where the policy admits them and the
    /// node opted in — and only from `Submission` to `Evaluating`, never for a class already held.
    #[test]
    fn a_node_prefetches_the_adapter_over_its_parent_and_full_weights_only_where_admitted() {
        let node = PalwImproveNodeV1 { holds: [h(HEAD)].into(), evaluates: false, prefetch_full: false };
        let prefetches = |rows: &PalwImproveRowsV1, node: &PalwImproveNodeV1| -> Vec<(Hash64, PalwImprovePrefetchV1)> {
            palw_improve_duties_v1(rows, node, 150, 64)
                .into_iter()
                .filter_map(|d| match d {
                    PalwImproveDutyV1::Prefetch { class_id, plan, .. } => Some((class_id, plan)),
                    _ => None,
                })
                .collect()
        };
        let rows = rows(PalwEpochStateV1::Submission, false);
        assert_eq!(
            prefetches(&rows, &node),
            vec![(
                h(0xC1),
                PalwImprovePrefetchV1::Adapter { parent_class: h(HEAD), parent_root: h(0x400), adapter_root: h(0xAD), p: 40 }
            )],
            "the adapter section only; the full-weight candidate is not admitted"
        );
        let full = rows_full();
        let opted = PalwImproveNodeV1 { prefetch_full: true, ..node.clone() };
        assert_eq!(prefetches(&full, &node).len(), 1, "full weights only for a node that opted in");
        assert_eq!(prefetches(&full, &opted)[1], (h(0xC2), PalwImprovePrefetchV1::Full { root: h(0xF0) }));
        let stranger = PalwImproveNodeV1 { holds: BTreeSet::new(), ..opted.clone() };
        assert_eq!(prefetches(&full, &stranger), vec![(h(0xC2), PalwImprovePrefetchV1::Full { root: h(0xF0) })], "no parent held");
        let held = PalwImproveNodeV1 { holds: [h(HEAD), h(0xC1)].into(), ..node.clone() };
        assert!(prefetches(&rows, &held).is_empty(), "a class already held is not fetched again");
        for (state, wants) in [
            (PalwEpochStateV1::Open, false),
            (PalwEpochStateV1::Submission, true),
            (PalwEpochStateV1::HoldOut, true),
            (PalwEpochStateV1::Drawn, true),
            (PalwEpochStateV1::Evaluating, true),
            (PalwEpochStateV1::Scoring, false),
            (PalwEpochStateV1::Decided, false),
        ] {
            assert_eq!(!prefetches(&rows_in(state), &node).is_empty(), wants, "{state:?}");
        }
    }

    fn rows_full() -> PalwImproveRowsV1 {
        rows(PalwEpochStateV1::Submission, true)
    }

    fn rows_in(state: PalwEpochStateV1) -> PalwImproveRowsV1 {
        rows(state, false)
    }

    /// **An evaluating node runs every unclaimed job it can**: in an `Evaluating` epoch before
    /// `t_eval`, each item for each subject whose class it holds (the parent, then candidates in
    /// acceptance order), derived as the chain derives it — id, mode per case, the item's seed, the
    /// policy's budget and stop ids, the pipeline of the case's scoring kind; a claimed job is skipped,
    /// the budget bounds the list, and nothing is planned past `t_eval` or in any other state.
    #[test]
    fn an_evaluating_node_plans_every_unclaimed_job_it_can_run_as_the_chain_derives_it() {
        let mut rows = rows(PalwEpochStateV1::Evaluating, false);
        let node = PalwImproveNodeV1 { holds: [h(HEAD), h(0xC1)].into(), evaluates: true, prefetch_full: false };
        let evaluations = |rows: &PalwImproveRowsV1, daa: u64, max: usize| -> Vec<(PalwEvalJobV1, Hash64)> {
            palw_improve_duties_v1(rows, &node, daa, max)
                .into_iter()
                .filter_map(|d| match d {
                    PalwImproveDutyV1::Evaluate { job, job_id, until_daa } => {
                        assert_eq!(until_daa, 280);
                        Some((job, job_id))
                    }
                    _ => None,
                })
                .collect()
        };
        let jobs = evaluations(&rows, 250, 64);
        assert_eq!(jobs.len(), 6, "three items × the parent and the held candidate");
        let seed = rows.epochs[&(h(LINE), 3)].seed.unwrap();
        for (job, id) in &jobs {
            assert_eq!(*id, palw_improve_eval_job_id_v1(&h(LINE), 3, job.item, &job.subject), "the chain's id");
            assert!(job.subject == PalwEvalSubjectV1::Parent || job.subject == PalwEvalSubjectV1::Candidate(h(0xC1)));
            match (job.item, &job.mode) {
                (0, PalwEvalModeV1::Generate { seed: s, max_new: 32, stop_ids }) => {
                    assert_eq!(*s, palw_improve_eval_seed_v1(&seed, 0));
                    assert_eq!(stop_ids, &vec![2]);
                    assert_eq!(job.pipeline_root, h(0xE1), "ExactMatch");
                }
                (1, PalwEvalModeV1::TeacherForced { reference_commitment }) => {
                    assert_eq!(*reference_commitment, h(0xB1));
                    assert_eq!(job.pipeline_root, h(0xE2), "RefLogLik");
                }
                (2, PalwEvalModeV1::Generate { .. }) => assert_eq!(job.pipeline_root, h(0xE3), "Judge"),
                other => panic!("unexpected job {other:?}"),
            }
        }
        assert_eq!(jobs[0].0.subject, PalwEvalSubjectV1::Parent, "the parent first");
        assert_eq!(evaluations(&rows, 250, 4).len(), 4, "the budget bounds the list");
        rows.claimed.insert(jobs[0].1);
        assert!(!evaluations(&rows, 250, 64).iter().any(|(_, id)| *id == jobs[0].1), "a claimed job is taken");
        assert!(evaluations(&rows, 280, 64).is_empty(), "nothing past t_eval");
        let idle = PalwImproveNodeV1 { evaluates: false, ..node.clone() };
        assert!(palw_improve_duties_v1(&rows, &idle, 250, 64).iter().all(|d| !matches!(d, PalwImproveDutyV1::Evaluate { .. })));
        for state in [PalwEpochStateV1::Drawn, PalwEpochStateV1::Scoring, PalwEpochStateV1::HoldOut] {
            let r = rows_in(state);
            assert!(palw_improve_duties_v1(&r, &node, 250, 64).iter().all(|d| !matches!(d, PalwImproveDutyV1::Evaluate { .. })));
        }
    }

    /// **A job is the chain's or none**: an item whose seed is not the epoch's, an epoch with no seed,
    /// and a case whose scoring kind the policy has no stage of derive no job; an item whose case the
    /// chain does not hold yet is skipped. The watcher's log notices each state change once.
    #[test]
    fn a_job_is_derived_only_from_the_epoch_s_own_seed_and_the_policy_s_stages() {
        let rows = rows(PalwEpochStateV1::Evaluating, false);
        let line = rows.line(&h(LINE)).unwrap();
        let epoch = rows.epoch(&h(LINE), 3).unwrap();
        let items = rows.items(&h(LINE), 3);
        let case = rows.case(&h(LINE), 3, &items[0]).unwrap();
        assert!(palw_improve_eval_job_v1(&line, &epoch, &items[0], PalwEvalSubjectV1::Parent, &case).is_ok());
        let mut forged = items[0].clone();
        forged.seed = h(0xF00);
        assert_eq!(
            palw_improve_eval_job_v1(&line, &epoch, &forged, PalwEvalSubjectV1::Parent, &case).unwrap_err(),
            "the item's seed is not the epoch's"
        );
        let unseeded = PalwImproveEpochViewV1 { seed: None, ..epoch.clone() };
        assert!(palw_improve_eval_job_v1(&line, &unseeded, &items[0], PalwEvalSubjectV1::Parent, &case).is_err());
        let mut narrow = line.clone();
        narrow.policy.eval.stages.retain(|s| s.kind != PalwScoringKindV1::ExactMatch);
        assert!(palw_improve_eval_job_v1(&narrow, &epoch, &items[0], PalwEvalSubjectV1::Parent, &case).is_err());
        let mut missing = rows.clone();
        missing.cases.remove(&h(0xCA0));
        let node = PalwImproveNodeV1 { holds: [h(HEAD)].into(), evaluates: true, prefetch_full: false };
        let planned: Vec<u32> = palw_improve_duties_v1(&missing, &node, 250, 64)
            .into_iter()
            .filter_map(|d| match d {
                PalwImproveDutyV1::Evaluate { job, .. } => Some(job.item),
                _ => None,
            })
            .collect();
        assert_eq!(planned, vec![1, 2], "items 1 and 2 for the parent: item 0's case is not on chain yet");
        // The watcher's log: a change once, then nothing.
        let e = |s| Some((3, s));
        assert_eq!(palw_improve_epoch_moved_v1(None, e(PalwEpochStateV1::Open)), e(PalwEpochStateV1::Open));
        assert_eq!(palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Open), e(PalwEpochStateV1::Open)), None);
        assert_eq!(
            palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Open), e(PalwEpochStateV1::Submission)),
            e(PalwEpochStateV1::Submission)
        );
        assert_eq!(palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Vesting), None), None);
    }
}
