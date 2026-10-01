//! **RFC-0004's node side (work item A10): what a node does for a governed line's epochs.**
//!
//! The chain decides everything an epoch does (spec 17 §17.5): its transitions happen at DAA
//! boundaries, its items are drawn by R, its scores are the committed outputs of evaluation claims, and
//! the fold promotes. A node's part is to *serve* that machine:
//!
//! * **watch** every governed line's open epoch — [`palw_improve_duties_v1`] reads the chain through
//!   [`PalwImproveChainV1`] and says what this node should do now;
//! * **prefetch** each candidate it can hold — a composite candidate over a parent it holds costs
//!   only the adapter section (§6.7); full weights only where the policy admits them and the node
//!   opted in ([`palw_improve_prefetch_plan_v1`]);
//! * **evaluate** — every `(item, subject)` task of an `Evaluating` epoch whose subject class it holds
//!   and that no claim has taken ([`palw_improve_eval_task_v1`]: the item's seed is the epoch's, the
//!   subjects the chain's — the parent, the candidates in acceptance order, the regression check's
//!   predecessor), run by the evaluation executor ([`crate::improve_eval`]).
//!
//! **Against interfaces, not the row layout.** Everything here reads the chain through
//! [`PalwImproveChainV1`]; [`PalwImproveStateChainV1`] implements it over the chain state's readers
//! (`improvement_line`, `improvement_policy`, `improvement_epoch`, `improvement_candidates`,
//! `improvement_items`, `improvement_subjects`), and [`PalwImproveMemChainV1`] in memory for tools and
//! tests.
//!
//! **The job's type and id are the evaluation lane's (A6)**: a task carries what the job is derived
//! from (line, epoch, item, subject and its class, the scoring kind, the mode) and the id the chain
//! keys it by ([`palw_improve_task_job_id_v1`], the one place that computes it); the kind a case is
//! scored by ([`palw_improve_scoring_kind_of_v1`]) and the mode are provisional until A6's derivation.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_material_v1::PalwCaseReferenceV1;
use kaspa_consensus_core::palw_improve_eval_v1::{
    PalwEvalJobV1, PalwEvalModeV1, PalwEvalStageParamsV1, palw_improve_eval_budget_positions_v1, palw_improve_eval_epoch_jobs_v1,
    palw_improve_eval_job_id_v1, palw_improve_eval_job_position_cap_v1, palw_improve_eval_mode_v1, palw_improve_eval_positions_v1,
};
use kaspa_consensus_core::palw_improve_v1::PalwImprovementCeilingsV1;
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwEpochCandidateV1, PalwEpochStateV1, PalwEpochTimesV1, PalwEvalItemV1, PalwEvalSubjectV1, PalwImprovementEpochViewV1,
    PalwImprovementPolicyV1, PalwScoringKindV1, palw_improve_eval_seed_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;

// ---------------------------------------------------------------------------------------------
// The chain, as a node reads it
// ---------------------------------------------------------------------------------------------

/// **A governed line, as the watcher reads it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImproveLineViewV1 {
    pub line_id: Hash64,
    /// The line's head: an IR class id (spec 17 §17.4.1).
    pub head: Hash64,
    /// The policy in force.
    pub policy: PalwImprovementPolicyV1,
    /// The epoch the line runs now, if any (one at a time, PALW-MIP-5).
    pub open_epoch: Option<u64>,
}

/// **An epoch, as the watcher reads it** — its header (state, clock, parent, the regression check's
/// predecessor, seed) and its candidates in acceptance order. Its items are read by key
/// ([`PalwImproveChainV1::items`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImproveEpochViewV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub state: PalwEpochStateV1,
    pub times: PalwEpochTimesV1,
    /// The head when the epoch opened: every candidate's parent, and the `Parent` subject's class.
    pub parent: Hash64,
    /// The head's predecessor when the epoch runs the regression check: the `Previous` subject.
    pub previous: Option<Hash64>,
    /// In acceptance order (the order ties go by, §17.9).
    pub candidates: Vec<PalwEpochCandidateV1>,
    /// The epoch seed, from `Drawing` on.
    pub seed: Option<Hash64>,
}

impl PalwImproveEpochViewV1 {
    /// **An epoch as the watcher reads it, from the read door's view** (header, candidates, seed).
    pub fn of_view_v1(v: &PalwImprovementEpochViewV1) -> Self {
        let header = &v.epoch;
        Self {
            line_id: header.line_id,
            epoch: header.epoch,
            state: header.state,
            times: header.times,
            parent: header.parent,
            previous: header.previous,
            candidates: v.candidates.clone(),
            seed: header.seed,
        }
    }
}

/// **What an item evaluates** — its prompt and its reference (a key's or a continuation's commitment,
/// or none for a judged case), whatever its source (a hold-out hard case, a setter's revealed prompt,
/// a suite's item).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvalCaseViewV1 {
    pub prompt_ids: Vec<u32>,
    pub reference: PalwCaseReferenceV1,
    /// The disclosed continuation of a likelihood item (RFC-0004 §7.1: from the draw on, as the
    /// material lane opens it) — the stream a teacher-forced job's prefill carries. `None` while the
    /// reference is still committed only.
    pub reference_ids: Option<Vec<u32>>,
    pub domain: u16,
}

/// **What the node reads of the chain** — the only door the watcher and the executor take into the
/// improvement tables.
pub trait PalwImproveChainV1 {
    /// Every governed line, by id.
    fn governed_lines(&self) -> Vec<Hash64>;
    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1>;
    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1>;
    /// The epoch's drawn items, in item order (empty before `Drawing` ends).
    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1>;
    /// The case item `item` of the epoch evaluates, when the chain holds it (a setter's prompt only
    /// once revealed) — the candidates lane's reader.
    fn case(&self, line_id: &Hash64, epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1>;
    /// Whether evaluation job `job_id` is taken: the first valid claim per job is the one (§17.8) —
    /// the evaluation lane's reader.
    fn job_claimed(&self, job_id: &Hash64) -> bool;
}

/// **The chain state's readers as a chain view** (the core lane's keyed layout, spec 17 §17.3): the
/// line header and its policy record, the epoch header, the candidates and items tables. The cases
/// and the claimed jobs are the candidates' and the evaluation lanes' readers, supplied until they
/// land beside these.
pub struct PalwImproveStateChainV1<'a> {
    pub state: &'a PalwChainStateV2,
    /// The DAA the lines are asked at (governance ends at an opt-out's effective height).
    pub daa: u64,
    pub case: &'a dyn Fn(&Hash64, u64, &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1>,
    pub claimed: &'a dyn Fn(&Hash64) -> bool,
    /// The lines to read (the state keeps no public index of them; the read door lists the open ones).
    pub lines: Vec<Hash64>,
}

impl PalwImproveChainV1 for PalwImproveStateChainV1<'_> {
    fn governed_lines(&self) -> Vec<Hash64> {
        self.lines.iter().filter(|line| self.state.improvement_governed_at(line, self.daa)).copied().collect()
    }

    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1> {
        let row = self.state.improvement_line(line_id)?;
        let policy = self.state.improvement_policy(line_id)?.clone();
        Some(PalwImproveLineViewV1 { line_id: row.line_id, head: row.head, policy, open_epoch: row.open_epoch })
    }

    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1> {
        let header = self.state.improvement_epoch(line_id, epoch)?;
        Some(PalwImproveEpochViewV1 {
            line_id: header.line_id,
            epoch: header.epoch,
            state: header.state,
            times: header.times,
            parent: header.parent,
            previous: header.previous,
            candidates: self.state.improvement_candidates(line_id, epoch).into_iter().map(|(_, row)| row.clone()).collect(),
            seed: header.seed,
        })
    }

    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1> {
        self.state.improvement_items(line_id, epoch).into_iter().copied().collect()
    }

    fn case(&self, line_id: &Hash64, epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1> {
        (self.case)(line_id, epoch, item)
    }

    fn job_claimed(&self, job_id: &Hash64) -> bool {
        (self.claimed)(job_id)
    }
}

/// **The node's read door as a chain view** (`ConsensusApi::palw_improvement_open_epochs_v1`, the core
/// lane's A9): one view per open epoch — the line's header and policy, the epoch's header, its
/// candidates in acceptance order and its items in item order. The door lists open epochs only, so a
/// line is governed here exactly while it has one (the watcher serves nothing else); the cases and the
/// claimed jobs come through the readers the candidates' and the evaluation lanes supply.
pub struct PalwImproveViewsChainV1<'a> {
    pub views: &'a [PalwImprovementEpochViewV1],
    pub case: &'a dyn Fn(&Hash64, u64, &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1>,
    pub claimed: &'a dyn Fn(&Hash64) -> bool,
}

impl PalwImproveViewsChainV1<'_> {
    fn view(&self, line_id: &Hash64) -> Option<&PalwImprovementEpochViewV1> {
        self.views.iter().find(|v| v.line.line_id == *line_id)
    }
}

impl PalwImproveChainV1 for PalwImproveViewsChainV1<'_> {
    fn governed_lines(&self) -> Vec<Hash64> {
        let mut lines: Vec<Hash64> = Vec::with_capacity(self.views.len());
        for v in self.views {
            if !lines.contains(&v.line.line_id) {
                lines.push(v.line.line_id);
            }
        }
        lines
    }

    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1> {
        let v = self.view(line_id)?;
        Some(PalwImproveLineViewV1 {
            line_id: v.line.line_id,
            head: v.line.head,
            policy: v.policy.clone(),
            open_epoch: v.line.open_epoch,
        })
    }

    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1> {
        let v = self.view(line_id).filter(|v| v.epoch.epoch == epoch)?;
        Some(PalwImproveEpochViewV1::of_view_v1(v))
    }

    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1> {
        self.view(line_id).filter(|v| v.epoch.epoch == epoch).map(|v| v.items.clone()).unwrap_or_default()
    }

    fn case(&self, line_id: &Hash64, epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1> {
        (self.case)(line_id, epoch, item)
    }

    fn job_claimed(&self, job_id: &Hash64) -> bool {
        (self.claimed)(job_id)
    }
}

/// **The node's read doors as one chain view** (`palw_improvement_open_epochs_v1` and the node lane's
/// `palw_improvement_eval_views_v1`): the open epochs' headers, candidates and items, and — from the
/// evaluation view — each item's disclosed prompt and reference and the jobs that already hold a live
/// claim. A job is *claimed* while a claim holds it that is not voided (a voided claim frees it, and the
/// fold's own rule: the first valid claim per job in the accepting chain's order is the one).
pub struct PalwImproveNodeChainV1<'a> {
    pub views: &'a [PalwImprovementEpochViewV1],
    pub eval: &'a [kaspa_consensus_core::palw_improve_node_v1::PalwImprovementEvalViewV1],
}

impl PalwImproveNodeChainV1<'_> {
    fn eval_view(&self, line_id: &Hash64, epoch: u64) -> Option<&kaspa_consensus_core::palw_improve_node_v1::PalwImprovementEvalViewV1> {
        self.eval.iter().find(|v| v.line_id == *line_id && v.epoch == epoch)
    }
}

impl PalwImproveChainV1 for PalwImproveNodeChainV1<'_> {
    fn governed_lines(&self) -> Vec<Hash64> {
        PalwImproveViewsChainV1 { views: self.views, case: &|_, _, _| None, claimed: &|_| false }.governed_lines()
    }

    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1> {
        PalwImproveViewsChainV1 { views: self.views, case: &|_, _, _| None, claimed: &|_| false }.line(line_id)
    }

    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1> {
        PalwImproveViewsChainV1 { views: self.views, case: &|_, _, _| None, claimed: &|_| false }.epoch(line_id, epoch)
    }

    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1> {
        PalwImproveViewsChainV1 { views: self.views, case: &|_, _, _| None, claimed: &|_| false }.items(line_id, epoch)
    }

    fn case(&self, line_id: &Hash64, epoch: u64, item: &PalwEvalItemV1) -> Option<PalwEvalCaseViewV1> {
        let view = self.eval_view(line_id, epoch)?.items.iter().find(|i| i.item == item.item)?;
        Some(PalwEvalCaseViewV1 {
            prompt_ids: view.prompt_ids.clone()?,
            reference: view.reference,
            reference_ids: view.reference_ids.clone(),
            domain: view.domain,
        })
    }

    fn job_claimed(&self, job_id: &Hash64) -> bool {
        self.eval.iter().flat_map(|v| &v.jobs).any(|j| j.job.id() == *job_id && j.claim.as_ref().is_some_and(|c| !c.voided))
    }
}

/// **A chain view in memory** — for tools and tests: lines, epochs, items and cases by id, the
/// claimed jobs.
#[derive(Clone, Debug, Default)]
pub struct PalwImproveMemChainV1 {
    pub lines: BTreeMap<Hash64, PalwImproveLineViewV1>,
    pub epochs: BTreeMap<(Hash64, u64), PalwImproveEpochViewV1>,
    pub items: BTreeMap<(Hash64, u64), Vec<PalwEvalItemV1>>,
    pub cases: BTreeMap<Hash64, PalwEvalCaseViewV1>,
    pub claimed: BTreeSet<Hash64>,
}

impl PalwImproveChainV1 for PalwImproveMemChainV1 {
    fn governed_lines(&self) -> Vec<Hash64> {
        self.lines.keys().copied().collect()
    }
    fn line(&self, line_id: &Hash64) -> Option<PalwImproveLineViewV1> {
        self.lines.get(line_id).cloned()
    }
    fn epoch(&self, line_id: &Hash64, epoch: u64) -> Option<PalwImproveEpochViewV1> {
        self.epochs.get(&(*line_id, epoch)).cloned()
    }
    fn items(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalItemV1> {
        self.items.get(&(*line_id, epoch)).cloned().unwrap_or_default()
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
    /// **The classes the chain admits claims of now** (RFC-0004 §17.8.4, A6-4: an evaluation claim is an
    /// FP claim of its subject's class, refused `ClassNotAdmitting` until the registry's lifecycle —
    /// Candidate → Prefetching → Probation — has taken the class to a state that admits claims). `None`
    /// where the registry does not govern: every class is then admitted. A node plans no evaluation for a
    /// subject whose class is not in it, so no run is spent on a claim the chain would refuse.
    pub admitting: Option<BTreeSet<Hash64>>,
    /// **The network's ceilings** (`Params::palw_improvement_v1`): with the line's policy they fix each
    /// evaluation job's share of the epoch's position budget (MIP-20, [`palw_improve_job_cap_v1`]).
    /// `None` where unknown: no job is skipped for its size, and the chain's refusal is then the answer.
    pub ceilings: Option<PalwImprovementCeilingsV1>,
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
// Evaluation tasks (spec 17 §17.8)
// ---------------------------------------------------------------------------------------------

/// **The subjects of an epoch**, in the chain's order (`improvement_subjects`): the parent, every
/// candidate in acceptance order, then the regression check's predecessor — with the class each runs.
pub fn palw_improve_subjects_v1(epoch: &PalwImproveEpochViewV1) -> Vec<(PalwEvalSubjectV1, Hash64)> {
    std::iter::once((PalwEvalSubjectV1::Parent, epoch.parent))
        .chain(epoch.candidates.iter().map(|c| (PalwEvalSubjectV1::Candidate(c.class_id), c.class_id)))
        .chain(epoch.previous.map(|previous| (PalwEvalSubjectV1::Previous(previous), previous)))
        .collect()
}

/// **The positions one evaluation job of `epoch` may take** (spec 17 §17.8.2, MIP-20): the fold's rule —
/// the epoch's budget (the smaller of the policy's `max_eval_positions` and the network's ceiling taken
/// at its permille) shared equally among the epoch's jobs (the policy's stages over the epoch's
/// subjects). A claim past it is refused whole, so a node does not carry one.
pub fn palw_improve_job_cap_v1(policy: &PalwImprovementPolicyV1, epoch: &PalwImproveEpochViewV1, ceilings: &PalwImprovementCeilingsV1) -> u64 {
    palw_improve_eval_job_position_cap_v1(
        palw_improve_eval_budget_positions_v1(policy.eval.max_eval_positions, ceilings),
        palw_improve_eval_epoch_jobs_v1(&policy.eval, palw_improve_subjects_v1(epoch).len()),
    )
}

/// **The scoring stage a case is scored by** (A6's mapping): an exact key by `ExactMatch`, a
/// continuation by `RefLogLik`, a case with no reference by a `Judge` (which this build cannot run yet,
/// [`palw_improve_kind_runnable_v1`]).
pub fn palw_improve_scoring_kind_of_v1(reference: &PalwCaseReferenceV1) -> PalwScoringKindV1 {
    match reference {
        PalwCaseReferenceV1::ExactKey { .. } => PalwScoringKindV1::ExactMatch,
        PalwCaseReferenceV1::Continuation { .. } => PalwScoringKindV1::RefLogLik,
        PalwCaseReferenceV1::None => PalwScoringKindV1::Judge,
    }
}

/// **An evaluation task**: what the job is derived from, the id the chain keys it by, and what a run
/// needs besides the subject's weights — the item's prompt, a likelihood item's disclosed reference,
/// and the scoring stage's parameters as the policy fixes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImproveEvalTaskV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub item: u32,
    pub subject: PalwEvalSubjectV1,
    /// The IR class the subject stage runs.
    pub subject_class: Hash64,
    pub kind: PalwScoringKindV1,
    pub mode: PalwEvalModeV1,
    pub job_id: Hash64,
    /// The item's disclosed prompt (the chain holds it: a hold-out case's, or a setter's once
    /// revealed).
    pub prompt_ids: Vec<u32>,
    /// A teacher-forced item's disclosed continuation; empty for a generating job.
    pub reference_ids: Vec<u32>,
    /// The scoring stage's parameters, from the policy (what the evaluation context is derived with).
    pub params: PalwEvalStageParamsV1,
}

impl PalwImproveEvalTaskV1 {
    /// **The positions this job takes at the least**: its prompt, and a teacher-forced job's reference
    /// (the stream it is given). A generating job takes more by what it generates.
    pub fn positions_floor(&self) -> u64 {
        palw_improve_eval_positions_v1(self.prompt_ids.len(), self.reference_ids.len())
    }

    /// **The positions the finished job took**: its prompt and the stream's ids (what the chain counts).
    pub fn positions_of(&self, stream_len: usize) -> u64 {
        palw_improve_eval_positions_v1(self.prompt_ids.len(), stream_len)
    }

    /// **The evaluation job** the chain derives for this task (A6's type): its id is [`Self::job_id`].
    pub fn job(&self) -> PalwEvalJobV1 {
        PalwEvalJobV1 {
            line_id: self.line_id,
            epoch: self.epoch,
            item: self.item,
            subject: self.subject,
            kind: self.kind,
            // A task is never a judged part (a judged kind is not runnable, below): part 0, the only part of every
            // other kind.
            part: 0,
            mode: self.mode.clone(),
        }
    }
}

/// **The job id a task is claimed under** — the one place the node computes it (A6's formula, which
/// binds the kind and the part: a subject's primary and judge jobs share an item, and a judged score is several
/// claims). A task is part 0, the only part of every kind this build runs.
pub fn palw_improve_task_job_id_v1(
    line_id: &Hash64,
    epoch: u64,
    item: u32,
    subject: &PalwEvalSubjectV1,
    kind: PalwScoringKindV1,
) -> Hash64 {
    palw_improve_eval_job_id_v1(line_id, epoch, item, subject, kind, 0)
}

/// **The scoring stage's parameters for a kind, as the policy fixes them** — what the chain derives
/// the evaluation context with (`palw_improve_eval_fold_v1`'s `palw_improve_eval_stage_params_of_v1`);
/// the revealed key's length bound is no context's.
pub fn palw_improve_stage_params_of_v1(policy: &PalwImprovementPolicyV1, kind: PalwScoringKindV1) -> Option<PalwEvalStageParamsV1> {
    use kaspa_consensus_core::palw_improve_state_v1::PalwScoringParamsV1 as P;
    // A judged stage's include its judge specification's logit scale (the pass it is runs at the judge's scale): a
    // policy with a judged stage and no specification derives none (the chain's check refuses such a policy).
    policy.eval.stages.iter().find(|stage| stage.kind == kind).and_then(|stage| {
        Some(match stage.params {
            P::ExactMatch { open, close, .. } => PalwEvalStageParamsV1::ExactMatch { open, close },
            P::RefLogLik { logit_scale_q24 } => PalwEvalStageParamsV1::RefLogLik { logit_scale_q24 },
            P::Judge { lo, hi } => PalwEvalStageParamsV1::Judge { lo, hi, logit_scale_q24: policy.eval.judge.as_ref()?.logit_scale_q24 },
            P::Pairwise { margin } => {
                PalwEvalStageParamsV1::Pairwise { margin, logit_scale_q24: policy.eval.pairwise.as_ref()?.logit_scale_q24 }
            }
        })
    })
}

/// **Can this build run a job of `kind`?** A6 derives a context for every kind now — ExactMatch (the subject's
/// generation, scored by the fold at the key's reveal), RefLogLik, and a judged part (the judge class's
/// teacher-forced pass over the registered template filled with the item and the final generations it reads,
/// spec 17 §17.8.5). This node plans the first two only: a judged part needs the template's dataset content, the
/// judge class and the generations' ids (which ride in claims' payloads, not in the chain's state), none of which
/// a node's loop reads yet — so it plans none, and a policy with a judged stage is evaluated by the lanes it has.
pub const fn palw_improve_kind_runnable_v1(kind: PalwScoringKindV1) -> bool {
    matches!(kind, PalwScoringKindV1::ExactMatch | PalwScoringKindV1::RefLogLik)
}

/// **The task of `item` for `subject`, derived as the chain derives it**. The item's seed must be the
/// epoch's (`H(epoch seed ‖ item)`, the same for every subject, so pairing is exact); a dropped item
/// derives nothing; the kind follows the case, and the policy must score it; the mode is A6's
/// (`palw_improve_eval_mode_v1`): teacher-forced over a continuation, otherwise generated under the
/// item's seed with the policy's budget and stop ids; a judged kind waits (a build that cannot run it
/// plans nothing); a teacher-forced task needs the reference disclosed.
pub fn palw_improve_eval_task_v1(
    line: &PalwImproveLineViewV1,
    epoch: &PalwImproveEpochViewV1,
    item: &PalwEvalItemV1,
    subject: PalwEvalSubjectV1,
    subject_class: Hash64,
    case: &PalwEvalCaseViewV1,
) -> Result<PalwImproveEvalTaskV1, &'static str> {
    let epoch_seed = epoch.seed.ok_or("the epoch has no seed yet: its items are drawn at Drawing")?;
    if item.dropped {
        return Err("the item is dropped for every subject");
    }
    if item.seed != palw_improve_eval_seed_v1(&epoch_seed, item.item) {
        return Err("the item's seed is not the epoch's");
    }
    let eval = &line.policy.eval;
    let kind = palw_improve_scoring_kind_of_v1(&case.reference);
    let params = palw_improve_stage_params_of_v1(&line.policy, kind).ok_or("the policy's eval spec has no stage of the case's scoring kind")?;
    if !palw_improve_kind_runnable_v1(kind) {
        return Err("this build plans no judged part (spec 17 §17.8.5: it needs the template, the judge class and the generations a node does not read yet)");
    }
    let reference_commitment = match case.reference {
        PalwCaseReferenceV1::Continuation { commitment } => Some(commitment),
        PalwCaseReferenceV1::ExactKey { .. } | PalwCaseReferenceV1::None => None,
    };
    let mode = palw_improve_eval_mode_v1(kind, eval, item.seed, reference_commitment, item.judge).map_err(|_| "the item names no mode")?;
    let reference_ids = match &mode {
        PalwEvalModeV1::TeacherForced { .. } => case.reference_ids.clone().ok_or("the item's reference is not disclosed yet")?,
        _ => Vec::new(),
    };
    let job_id = palw_improve_task_job_id_v1(&line.line_id, epoch.epoch, item.item, &subject, kind);
    Ok(PalwImproveEvalTaskV1 {
        line_id: line.line_id,
        epoch: epoch.epoch,
        item: item.item,
        subject,
        subject_class,
        kind,
        mode,
        job_id,
        prompt_ids: case.prompt_ids.clone(),
        reference_ids,
        params,
    })
}

// ---------------------------------------------------------------------------------------------
// The watcher's plan
// ---------------------------------------------------------------------------------------------

/// **Something this node should do for a governed line now.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwImproveDutyV1 {
    /// Fetch a candidate's artifact (or its adapter section) before the epoch evaluates it.
    Prefetch { line_id: Hash64, epoch: u64, class_id: Hash64, plan: PalwImprovePrefetchV1 },
    /// Run an evaluation task and claim it before `until_daa` (the epoch's `t_eval`).
    Evaluate { task: PalwImproveEvalTaskV1, until_daa: u64 },
}

/// Do an epoch's candidates want prefetching in `state`? From the moment they are known (they are
/// submitted in `Submission`) until evaluation ends.
fn prefetches_in(state: PalwEpochStateV1) -> bool {
    matches!(
        state,
        PalwEpochStateV1::Submission | PalwEpochStateV1::HoldOut | PalwEpochStateV1::Drawing | PalwEpochStateV1::Evaluating
    )
}

/// **What this node should do now for every governed line** — pure, over the chain view:
///
/// * every candidate of an epoch between `Submission` and `Evaluating` that this node does not hold
///   and can fetch ([`palw_improve_prefetch_plan_v1`]);
/// * for an evaluating node, every task of an `Evaluating` epoch before its `t_eval` — each item for
///   each subject whose class this node holds — that no claim has taken, derived as the chain derives
///   it, in item order then subject order, at most `max_jobs`. An item whose case the chain does not
///   hold yet (a setter's prompt before its reveal), a dropped item, or one that derives no task is
///   skipped.
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
        let subjects: Vec<(PalwEvalSubjectV1, Hash64)> = palw_improve_subjects_v1(&epoch)
            .into_iter()
            .filter(|(_, class)| node.holds.contains(class) && node.admitting.as_ref().is_none_or(|admitting| admitting.contains(class)))
            .collect();
        let job_cap = node.ceilings.as_ref().map(|ceilings| palw_improve_job_cap_v1(&line.policy, &epoch, ceilings));
        for item in chain.items(&line_id, epoch.epoch) {
            let Some(case) = chain.case(&line_id, epoch.epoch, &item) else { continue };
            for (subject, class) in &subjects {
                if jobs >= max_jobs {
                    return duties;
                }
                let Ok(task) = palw_improve_eval_task_v1(&line, &epoch, &item, *subject, *class, &case) else { continue };
                if chain.job_claimed(&task.job_id) {
                    continue;
                }
                // A job whose floor is past its share of the epoch's budget can never be claimed (MIP-20).
                if job_cap.is_some_and(|cap| task.positions_floor() > cap) {
                    continue;
                }
                duties.push(PalwImproveDutyV1::Evaluate { task, until_daa: epoch.times.t_eval });
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

/// **Fixtures for the watcher's tests** (here and in kaspad): a governed line's epoch in a given state,
/// with a composite and a full-weight candidate, the regression check's predecessor and three items.
#[doc(hidden)]
pub mod testing {
    pub use super::tests_fixtures::*;
}

#[doc(hidden)]
mod tests_fixtures {
    use super::*;
    use kaspa_consensus_core::palw_improve_state_v1::{
        PALW_IMPROVEMENT_POLICY_VERSION_V1, PalwEpochWindowsV1, PalwEvalSpecV1, PalwImprovementFeesV1, PalwItemSourceV1,
        PalwProvenancePolicyV1, PalwScoringParamsV1, PalwScoringStageV1, PalwUsageMeasureV1, PalwUsageThresholdV1,
    };
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
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
                    PalwScoringStageV1 {
                        kind: PalwScoringKindV1::ExactMatch,
                        params: PalwScoringParamsV1::ExactMatch { open: -1, close: -1, key_cap: 8 },
                    },
                    PalwScoringStageV1 {
                        kind: PalwScoringKindV1::RefLogLik,
                        params: PalwScoringParamsV1::RefLogLik { logit_scale_q24: 1 << 20 },
                    },
                    PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: -100, hi: 100 } },
                ],
                regression_dataset: h(0),
                regression_items: 0,
                safety_dataset: h(0),
                safety_items: 0,
                judge_set: vec![h(0x7D)],
                judge: Some(kaspa_consensus_core::palw_improve_state_v1::PalwJudgeSpecV1 {
                    template_dataset: h(0x7E),
                    verdict_a: vec![1],
                    verdict_b: vec![2],
                    logit_scale_q24: 1 << 20,
                }),
                pairwise: None,
                seat_pool_permille: 100,
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
                max_eval_positions: 1 << 20,
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
                s1_bounty: 1,
                s1_setter_reward: 1,
            },
            phi_permille: 100,
            bounty_share_permille: 100,
            promotion_share_permille: 100,
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
    pub const PREVIOUS: u64 = 0x9E7;
    pub const LINE: u64 = 0x11E;

    pub fn candidate(class: u64, artifact: PalwTirArtifactRefV1, n: u64) -> PalwEpochCandidateV1 {
        PalwEpochCandidateV1 {
            class_id: h(class),
            submitter: bond(n),
            artifact,
            declarations_digest: h(0),
            datasets: vec![],
            fee_paid: 1,
            bond: 1,
            escrow: 10,
            escrow_spent: 0,
            submitted_daa: 160 + n,
            counts: None,
        }
    }

    /// A governed line whose epoch 3 is in `state`, with a composite candidate over the head and a
    /// full-weight one, the regression check's predecessor, and three drawn items: an exact key, a
    /// continuation, a judged case.
    pub fn chain(state: PalwEpochStateV1, full_weights: bool) -> PalwImproveMemChainV1 {
        let seed = h(0x5EED);
        let composite = PalwTirArtifactRefV1::Composite { parent_class: h(HEAD), parent_root: h(0x400), adapter_root: h(0xAD), p: 40 };
        let item = |i: u32, case: u64| PalwEvalItemV1 {
            item: i,
            case_id: h(case),
            source: PalwItemSourceV1::HoldOut,
            supplier: Some(bond(9)),
            seed: palw_improve_eval_seed_v1(&seed, i),
            judge: None,
            dropped: false,
        };
        let line = PalwImproveLineViewV1 { line_id: h(LINE), head: h(HEAD), policy: policy(full_weights), open_epoch: Some(3) };
        let epoch = PalwImproveEpochViewV1 {
            line_id: h(LINE),
            epoch: 3,
            state,
            times: PalwEpochTimesV1 { t_open: 100, t_fix: 150, t_close: 200, t_draw: 220, t_eval: 280, t_score: 290 },
            parent: h(HEAD),
            previous: Some(h(PREVIOUS)),
            candidates: vec![candidate(0xC1, composite, 2), candidate(0xC2, PalwTirArtifactRefV1::Single { root: h(0xF0) }, 3)],
            seed: Some(seed),
        };
        let case = |reference: PalwCaseReferenceV1| PalwEvalCaseViewV1 {
            prompt_ids: vec![1, 5, 9],
            reference_ids: matches!(reference, PalwCaseReferenceV1::Continuation { .. }).then(|| vec![6, 7]),
            reference,
            domain: 0,
        };
        PalwImproveMemChainV1 {
            lines: [(h(LINE), line)].into(),
            epochs: [((h(LINE), 3), epoch)].into(),
            items: [((h(LINE), 3), vec![item(0, 0xCA0), item(1, 0xCA1), item(2, 0xCA2)])].into(),
            cases: [
                (h(0xCA0), case(PalwCaseReferenceV1::ExactKey { commitment: h(0xB0) })),
                (h(0xCA1), case(PalwCaseReferenceV1::Continuation { commitment: h(0xB1) })),
                (h(0xCA2), case(PalwCaseReferenceV1::None)),
            ]
            .into(),
            claimed: BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_fixtures::*;
    use super::*;

    fn prefetches(chain: &PalwImproveMemChainV1, node: &PalwImproveNodeV1) -> Vec<(Hash64, PalwImprovePrefetchV1)> {
        palw_improve_duties_v1(chain, node, 150, 64)
            .into_iter()
            .filter_map(|d| match d {
                PalwImproveDutyV1::Prefetch { class_id, plan, .. } => Some((class_id, plan)),
                _ => None,
            })
            .collect()
    }

    fn tasks(chain: &PalwImproveMemChainV1, node: &PalwImproveNodeV1, daa: u64, max: usize) -> Vec<PalwImproveEvalTaskV1> {
        palw_improve_duties_v1(chain, node, daa, max)
            .into_iter()
            .filter_map(|d| match d {
                PalwImproveDutyV1::Evaluate { task, until_daa } => {
                    assert_eq!(until_daa, 280);
                    Some(task)
                }
                _ => None,
            })
            .collect()
    }

    /// **Prefetch follows the artifact**: a composite candidate over a held parent is its adapter
    /// section; over a parent not held, nothing; full weights only where the policy admits them and the
    /// node opted in — and only from `Submission` to `Evaluating`, never for a class already held.
    #[test]
    fn a_node_prefetches_the_adapter_over_its_parent_and_full_weights_only_where_admitted() {
        let node = PalwImproveNodeV1 { holds: [h(HEAD)].into(), evaluates: false, prefetch_full: false, admitting: None, ceilings: None };
        let submission = chain(PalwEpochStateV1::Submission, false);
        assert_eq!(
            prefetches(&submission, &node),
            vec![(
                h(0xC1),
                PalwImprovePrefetchV1::Adapter { parent_class: h(HEAD), parent_root: h(0x400), adapter_root: h(0xAD), p: 40 }
            )],
            "the adapter section only; the full-weight candidate is not admitted"
        );
        let full = chain(PalwEpochStateV1::Submission, true);
        let opted = PalwImproveNodeV1 { prefetch_full: true, ..node.clone() };
        assert_eq!(prefetches(&full, &node).len(), 1, "full weights only for a node that opted in");
        assert_eq!(prefetches(&full, &opted)[1], (h(0xC2), PalwImprovePrefetchV1::Full { root: h(0xF0) }));
        let stranger = PalwImproveNodeV1 { holds: BTreeSet::new(), ..opted.clone() };
        assert_eq!(prefetches(&full, &stranger), vec![(h(0xC2), PalwImprovePrefetchV1::Full { root: h(0xF0) })], "no parent held");
        let held = PalwImproveNodeV1 { holds: [h(HEAD), h(0xC1)].into(), ..node.clone() };
        assert!(prefetches(&submission, &held).is_empty(), "a class already held is not fetched again");
        for (state, wants) in [
            (PalwEpochStateV1::Open, false),
            (PalwEpochStateV1::Submission, true),
            (PalwEpochStateV1::HoldOut, true),
            (PalwEpochStateV1::Drawing, true),
            (PalwEpochStateV1::Evaluating, true),
            (PalwEpochStateV1::Closing, false),
            (PalwEpochStateV1::Decided, false),
        ] {
            assert_eq!(!prefetches(&chain(state, false), &node).is_empty(), wants, "{state:?}");
        }
    }

    /// **An evaluating node runs every unclaimed task it can**: in an `Evaluating` epoch before
    /// `t_eval`, each item for each subject whose class it holds — the parent, the candidates in
    /// acceptance order, the regression check's predecessor — derived as the chain derives it (the
    /// chain's id, the mode per case, the item's seed, the policy's budget and stop ids, the kind of the
    /// case); a claimed task is skipped, the budget bounds the list, and nothing is planned past `t_eval`
    /// or in any other state.
    #[test]
    fn an_evaluating_node_plans_every_unclaimed_task_it_can_run_as_the_chain_derives_it() {
        let mut chain = chain(PalwEpochStateV1::Evaluating, false);
        let node = PalwImproveNodeV1 { holds: [h(HEAD), h(0xC1), h(PREVIOUS)].into(), evaluates: true, prefetch_full: false, admitting: None, ceilings: None };
        let planned = tasks(&chain, &node, 250, 64);
        assert_eq!(planned.len(), 6, "two runnable items × the parent, the held candidate and the predecessor (the judged item waits)");
        let seed = chain.epochs[&(h(LINE), 3)].seed.unwrap();
        for task in &planned {
            assert_eq!(task.job_id, palw_improve_eval_job_id_v1(&h(LINE), 3, task.item, &task.subject, task.kind, 0), "the chain's id");
            assert_eq!(task.job().id(), task.job_id, "A6's job type derives the same id");
            assert_eq!(task.prompt_ids, vec![1, 5, 9]);
            let expected_class = match task.subject {
                PalwEvalSubjectV1::Parent => h(HEAD),
                PalwEvalSubjectV1::Candidate(c) => c,
                PalwEvalSubjectV1::Previous(p) => p,
            };
            assert_eq!(task.subject_class, expected_class);
            match (task.item, &task.mode) {
                (0, PalwEvalModeV1::Generate { seed: s, max_new: 32, stop_ids }) => {
                    assert_eq!(*s, palw_improve_eval_seed_v1(&seed, 0));
                    assert_eq!(stop_ids, &vec![2]);
                    assert_eq!(task.kind, PalwScoringKindV1::ExactMatch);
                    assert_eq!(task.params, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 });
                }
                (1, PalwEvalModeV1::TeacherForced { reference_commitment }) => {
                    assert_eq!(*reference_commitment, h(0xB1));
                    assert_eq!(task.kind, PalwScoringKindV1::RefLogLik);
                    assert_eq!(task.reference_ids, vec![6, 7], "the disclosed continuation rides the task");
                    assert_eq!(task.params, PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 20 });
                }
                other => panic!("unexpected task {other:?}"),
            }
        }
        assert_eq!(
            planned[..3].iter().map(|t| t.subject).collect::<Vec<_>>(),
            vec![PalwEvalSubjectV1::Parent, PalwEvalSubjectV1::Candidate(h(0xC1)), PalwEvalSubjectV1::Previous(h(PREVIOUS))],
            "the chain's subject order"
        );
        assert_eq!(tasks(&chain, &node, 250, 4).len(), 4, "the budget bounds the list");
        chain.claimed.insert(planned[0].job_id);
        assert!(!tasks(&chain, &node, 250, 64).iter().any(|t| t.job_id == planned[0].job_id), "a claimed task is taken");
        assert!(tasks(&chain, &node, 280, 64).is_empty(), "nothing past t_eval");
        // A subject whose class the chain does not admit claims of yet (the registry's lifecycle) is not planned.
        let only_head = PalwImproveNodeV1 { admitting: Some([h(HEAD)].into()), ..node.clone() };
        let planned_head = tasks(&chain, &only_head, 250, 64);
        assert!(
            planned_head.iter().all(|t| t.subject_class == h(HEAD)) && planned_head.iter().map(|t| t.item).collect::<Vec<_>>() == vec![1],
            "the parent's item 1 only (item 0 is claimed above): {planned_head:?}"
        );
        let none_admitting = PalwImproveNodeV1 { admitting: Some(BTreeSet::new()), ..node.clone() };
        assert!(tasks(&chain, &none_admitting, 250, 64).is_empty());
        let idle = PalwImproveNodeV1 { evaluates: false, ..node.clone() };
        assert!(tasks(&chain, &idle, 250, 64).is_empty());
        for state in [PalwEpochStateV1::Drawing, PalwEpochStateV1::Closing, PalwEpochStateV1::HoldOut] {
            assert!(tasks(&self::chain(state, false), &node, 250, 64).is_empty(), "{state:?}");
        }
    }

    /// **A job past its share of the epoch's position budget is not planned** (MIP-20): the fold's cap —
    /// the epoch's budget over its jobs (the policy's stages over the epoch's subjects) — held against the
    /// job's floor (its prompt, and a teacher-forced job's reference); the exact count is the run's.
    #[test]
    fn a_job_whose_floor_is_past_its_share_of_the_epoch_s_position_budget_is_not_planned() {
        let chain = chain(PalwEpochStateV1::Evaluating, false);
        let line = chain.line(&h(LINE)).unwrap();
        let epoch = chain.epoch(&h(LINE), 3).unwrap();
        // 8 items × (1 primary + the Judge stage's 2 parts) × 4 subjects (the parent, two candidates, the predecessor), no suite entries = 96 jobs.
        let jobs = palw_improve_eval_epoch_jobs_v1(&line.policy.eval, 4);
        assert_eq!(jobs, 96, "the epoch's jobs");
        let capped = |share: u64| PalwImprovementCeilingsV1 { max_eval_positions_per_epoch: jobs * share, ..PalwImprovementCeilingsV1::FORMAT_CAPS_V1 };
        assert_eq!(palw_improve_job_cap_v1(&line.policy, &epoch, &capped(4)), 4);
        assert_eq!(palw_improve_job_cap_v1(&line.policy, &epoch, &capped(10)), 10);
        let narrower = PalwImprovementCeilingsV1 { max_eval_budget_permille: 500, ..capped(10) };
        assert_eq!(palw_improve_job_cap_v1(&line.policy, &epoch, &narrower), 5, "the permille share of the ceiling");
        let node = |ceilings| PalwImproveNodeV1 {
            holds: [h(HEAD)].into(),
            evaluates: true,
            prefetch_full: false,
            admitting: None,
            ceilings,
        };
        let items = |ceilings| tasks(&chain, &node(ceilings), 250, 64).into_iter().map(|t| t.item).collect::<Vec<_>>();
        assert_eq!(items(None), vec![0, 1], "no ceilings known: nothing is skipped for its size");
        assert_eq!(items(Some(capped(5))), vec![0, 1], "the likelihood job's floor is 3 + 2 = 5: it fits");
        assert_eq!(items(Some(capped(4))), vec![0], "past a cap of 4 only the generating job's floor (the prompt, 3) is within it");
        assert!(items(Some(capped(2))).is_empty(), "no job fits a cap of 2");
        let task = &tasks(&chain, &node(None), 250, 64)[1];
        assert_eq!((task.positions_floor(), task.positions_of(2)), (5, 5), "the prompt and the reference");
    }

    /// **A task is the chain's or none**: an item whose seed is not the epoch's, a dropped item, an
    /// epoch with no seed, and a case whose scoring kind the policy has no stage of derive nothing; an
    /// item whose case the chain does not hold yet is skipped. The watcher's log notices each state
    /// change once.
    #[test]
    fn a_task_is_derived_only_from_the_epoch_s_own_seed_and_the_policy_s_stages() {
        let chain = chain(PalwEpochStateV1::Evaluating, false);
        let line = chain.line(&h(LINE)).unwrap();
        let epoch = chain.epoch(&h(LINE), 3).unwrap();
        let items = chain.items(&h(LINE), 3);
        let case = chain.case(&h(LINE), 3, &items[0]).unwrap();
        let task = |item: &PalwEvalItemV1, epoch: &PalwImproveEpochViewV1, line: &PalwImproveLineViewV1| {
            palw_improve_eval_task_v1(line, epoch, item, PalwEvalSubjectV1::Parent, h(HEAD), &case)
        };
        assert!(task(&items[0], &epoch, &line).is_ok());
        let mut forged = items[0];
        forged.seed = h(0xF00);
        assert_eq!(task(&forged, &epoch, &line).unwrap_err(), "the item's seed is not the epoch's");
        let mut dropped = items[0];
        dropped.dropped = true;
        assert_eq!(task(&dropped, &epoch, &line).unwrap_err(), "the item is dropped for every subject");
        let unseeded = PalwImproveEpochViewV1 { seed: None, ..epoch.clone() };
        assert!(task(&items[0], &unseeded, &line).is_err());
        let mut narrow = line.clone();
        narrow.policy.eval.stages.retain(|s| s.kind != PalwScoringKindV1::ExactMatch);
        assert!(task(&items[0], &epoch, &narrow).is_err());
        let mut missing = chain.clone();
        missing.cases.remove(&h(0xCA0));
        let node = PalwImproveNodeV1 { holds: [h(HEAD)].into(), evaluates: true, prefetch_full: false, admitting: None, ceilings: None };
        let planned: Vec<u32> = tasks(&missing, &node, 250, 64).into_iter().map(|t| t.item).collect();
        assert_eq!(planned, vec![1], "item 1 for the parent: item 0's case is not on chain yet, item 2 is judged and waits");
        // A likelihood item whose reference is not disclosed yet derives no task: its prefill needs it.
        let mut undisclosed = chain.clone();
        undisclosed.cases.get_mut(&h(0xCA1)).unwrap().reference_ids = None;
        assert_eq!(tasks(&undisclosed, &node, 250, 64).into_iter().map(|t| t.item).collect::<Vec<_>>(), vec![0]);
        // The watcher's log: a change once, then nothing.
        let e = |s| Some((3, s));
        assert_eq!(palw_improve_epoch_moved_v1(None, e(PalwEpochStateV1::Open)), e(PalwEpochStateV1::Open));
        assert_eq!(palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Open), e(PalwEpochStateV1::Open)), None);
        assert_eq!(
            palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Open), e(PalwEpochStateV1::Submission)),
            e(PalwEpochStateV1::Submission)
        );
        assert_eq!(palw_improve_epoch_moved_v1(e(PalwEpochStateV1::Decided), None), None);
    }

    /// **The chain state's readers as the view** (the core lane's keyed layout): a state carrying one
    /// governed line, its policy, its open epoch's header, candidates and items reads back through
    /// [`PalwImproveStateChainV1`] exactly as the rows say — the subjects in the chain's own order
    /// (`improvement_subjects`) — and plans the same tasks as the in-memory view; so does the node's
    /// read door ([`PalwImproveViewsChainV1`] over `improvement_open_epoch_views_v1`).
    #[test]
    fn the_state_readers_are_the_same_view_as_the_rows() {
        use kaspa_consensus_core::palw_improve_state_v1::{
            PalwEpochEscrowV1, PalwEpochRetireV1, PalwImprovementEpochV1, PalwImprovementLineStatusV1, PalwImprovementLineV1,
            PalwImprovementPolicyRecordV1, palw_improvement_policy_digest_v1,
        };
        use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
        let mem = chain(PalwEpochStateV1::Evaluating, false);
        let line = &mem.lines[&h(LINE)];
        let epoch = &mem.epochs[&(h(LINE), 3)];
        let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        carriage.improvement_lines.insert(
            h(LINE),
            PalwImprovementLineV1 {
                line_id: h(LINE),
                class_id: h(HEAD),
                policy_digest: palw_improvement_policy_digest_v1(&line.policy),
                policy_sequence: 1,
                status: PalwImprovementLineStatusV1::Governed,
                governed_from_daa: 0,
                head: h(HEAD),
                head_seq: 1,
                next_epoch: 4,
                open_epoch: Some(3),
                next_due_daa: 280,
                next_check_daa: 280,
                barred: vec![],
                last_promotion: None,
                regression_epoch: None,
                regression_check: None,
            },
        );
        carriage.improvement_policies.insert(h(LINE), PalwImprovementPolicyRecordV1 { policy: line.policy.clone(), pending: None });
        carriage.improvement_usage.insert(h(LINE), Default::default());
        carriage.improvement_pools.insert(h(LINE), Default::default());
        carriage.improvement_epochs.insert(
            (h(LINE), 3),
            PalwImprovementEpochV1 {
                line_id: h(LINE),
                epoch: 3,
                state: epoch.state,
                times: epoch.times,
                parent: epoch.parent,
                previous: epoch.previous,
                policy_digest: palw_improvement_policy_digest_v1(&line.policy),
                dataset_root: None,
                candidates: epoch.candidates.len() as u32,
                pool_entries: 0,
                holdout_cases: 0,
                setter_sets: 0,
                seed: epoch.seed,
                items: 3,
                results_bound: 0,
                previous_counts: None,
                outcome: None,
                escrow: PalwEpochEscrowV1::default(),
                grants: 0,
                decided_daa: None,
                retire: PalwEpochRetireV1::Pending,
            },
        );
        for (i, c) in epoch.candidates.iter().enumerate() {
            carriage.improvement_candidates.insert((h(LINE), 3, i as u32), c.clone());
        }
        for item in &mem.items[&(h(LINE), 3)] {
            carriage.improvement_items.insert((h(LINE), 3, item.item), *item);
        }
        let params =
            kaspa_consensus_core::palw_state_v2::PalwStateParamsV2::new(100, 10, 10, 20, 600, 1000, h(1), 4, 1000, 10_000, 1000, 0)
                .expect("params");
        let state = carriage.into_state(&params, None).expect("a consistent carriage");
        let case = |_: &Hash64, _: u64, item: &PalwEvalItemV1| mem.cases.get(&item.case_id).cloned();
        let claimed = |_: &Hash64| false;
        let view = PalwImproveStateChainV1 { state: &state, daa: 250, case: &case, claimed: &claimed, lines: vec![h(LINE)] };
        assert_eq!(view.governed_lines(), vec![h(LINE)]);
        assert_eq!(view.line(&h(LINE)).as_ref(), Some(line));
        assert_eq!(view.epoch(&h(LINE), 3).as_ref(), Some(epoch));
        assert_eq!(view.items(&h(LINE), 3), mem.items[&(h(LINE), 3)]);
        let subjects: Vec<PalwEvalSubjectV1> = palw_improve_subjects_v1(epoch).into_iter().map(|(s, _)| s).collect();
        assert_eq!(subjects, state.improvement_subjects(&h(LINE), 3), "the chain's own subject order");
        let node = PalwImproveNodeV1 { holds: [h(HEAD), h(0xC1), h(PREVIOUS)].into(), evaluates: true, prefetch_full: false, admitting: None, ceilings: None };
        assert_eq!(palw_improve_duties_v1(&view, &node, 250, 64), palw_improve_duties_v1(&mem, &node, 250, 64));
        // The node's read door (`improvement_open_epoch_views_v1`, what `ConsensusApi` serves) reads back
        // the same view, and plans the same.
        let views = state.improvement_open_epoch_views_v1();
        assert_eq!(views.len(), 1, "one open epoch");
        let door = PalwImproveViewsChainV1 { views: &views, case: &case, claimed: &claimed };
        assert_eq!(door.governed_lines(), vec![h(LINE)]);
        assert_eq!(door.line(&h(LINE)).as_ref(), Some(line));
        assert_eq!(door.epoch(&h(LINE), 3).as_ref(), Some(epoch));
        assert!(door.epoch(&h(LINE), 2).is_none(), "only the open epoch");
        assert_eq!(door.items(&h(LINE), 3), mem.items[&(h(LINE), 3)]);
        assert_eq!(palw_improve_duties_v1(&door, &node, 250, 64), palw_improve_duties_v1(&mem, &node, 250, 64));
    }
}
