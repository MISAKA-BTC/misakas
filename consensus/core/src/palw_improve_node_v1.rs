//! **RFC-0004 (work item A10): what a node reads of the improvement protocol** — the node lane's read
//! model beside the core's open-epoch door (`palw_improvement_open_epochs_v1`). Two doors, both pure
//! reads of the tip's chain state (never hashed, never carried to a peer):
//!
//! * **the evaluation view** ([`PalwImprovementEvalViewV1`]): for every open epoch that has drawn its
//!   items, each item's disclosed prompt and reference (a hold-out case's, or a setter's once revealed
//!   — what an evaluation job derives from), and every evaluation job that holds a claim — the job key,
//!   the claim and the roots it committed. A node plans its evaluations from the first, and a seat
//!   judges an evaluation claim from the second: the job, the prompt, the reference and the policy's
//!   parameters are all in the state, so a replay needs nothing from the executor but the claim's roots.
//! * **the status** ([`PalwImprovementStatusV1`]): every governed line — its header, usage and pool, its
//!   head history, the epoch rows the chain still keeps (decided ones included, with their outcome and
//!   escrow), their candidates and their grants. What an operator, a drill's watcher and a wallet read.
//!
//! Dormant: both are empty below `palw_improvement_v1` (no table holds a row).

use crate::Hash64;
use crate::palw_improve_eval_v1::PalwEvalJobKeyV1;
use crate::palw_improve_material_v1::PalwCaseReferenceV1;
use crate::palw_improve_state_v1::{
    PalwEpochCandidateV1, PalwEpochStateV1, PalwImprovementEpochV1, PalwImprovementLineV1, PalwImprovementPoolV1,
    PalwImprovementUsageV1, PalwItemSourceV1, PalwLineageHeadEntryV1, PalwRewardGrantV1,
};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2};

/// **One drawn item as an evaluation reads it**: the prompt (empty until the chain holds it: a setter's
/// before its reveal), the reference's kind and commitment, the disclosed continuation of a likelihood
/// item (from the draw on), and the case's domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementEvalItemViewV1 {
    pub item: u32,
    /// `None` while the chain does not hold the prompt (a setter's, before `SetterSetRevealed`).
    pub prompt_ids: Option<Vec<u32>>,
    pub reference: PalwCaseReferenceV1,
    /// A likelihood item's disclosed continuation; `None` while only committed.
    pub reference_ids: Option<Vec<u32>>,
    pub domain: u16,
    pub dropped: bool,
}

/// **The claim that took an evaluation job**, as the state holds it: its roots are what a seat's
/// replay is held to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementEvalClaimViewV1 {
    pub claim_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub accepted_daa: u64,
    pub final_daa: Option<u64>,
    /// The claim was voided: the job is free again.
    pub voided: bool,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub work_leaves: u64,
    /// The committed score a RefLogLik (or judged) claim recorded, when the row holds one.
    pub score: Option<i64>,
}

/// **An evaluation job that holds a claim**: its key (line, epoch, item, subject, kind, part) and its claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementEvalJobViewV1 {
    pub key: PalwEvalJobKeyV1,
    pub job: crate::palw_improve_eval_v1::PalwEvalJobV1,
    pub claim: Option<PalwImprovementEvalClaimViewV1>,
}

/// **An open epoch's evaluation view**: its drawn items (in item order) and the jobs that hold claims.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementEvalViewV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub items: Vec<PalwImprovementEvalItemViewV1>,
    pub jobs: Vec<PalwImprovementEvalJobViewV1>,
}

/// **One governed line's status**: everything the chain keeps of it a reader may ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementLineStatusV1 {
    pub line: PalwImprovementLineV1,
    pub usage: Option<PalwImprovementUsageV1>,
    pub pool: Option<PalwImprovementPoolV1>,
    /// The kept head history, oldest first, with each entry's sequence number.
    pub heads: Vec<(u32, PalwLineageHeadEntryV1)>,
    /// Every epoch row the chain still keeps, oldest first.
    pub epochs: Vec<PalwImprovementEpochStatusV1>,
}

/// One epoch row with its candidates and grants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwImprovementEpochStatusV1 {
    pub epoch: PalwImprovementEpochV1,
    pub candidates: Vec<PalwEpochCandidateV1>,
    pub grants: Vec<PalwRewardGrantV1>,
}

/// **The improvement protocol's status at the tip.**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwImprovementStatusV1 {
    pub lines: Vec<PalwImprovementLineStatusV1>,
    /// Every class admitted as a composite candidate with the reference it was admitted with (decision
    /// 7a), in class-id order — kept for as long as the class lives, which the epoch rows are not
    /// (RFC-0004 §6.7: a seat proves a composite's possession over its adapter section, which only this
    /// record names).
    pub composite_classes: Vec<(Hash64, crate::palw_improve_composite_v1::PalwTirCompositeRefV1)>,
}

impl PalwChainStateV2 {
    /// **An item's prompt and reference, as the chain discloses them** (the evaluation lane's hooks'
    /// own sources): a hold-out case's prompt ids and its reference — a `Continuation`'s ids once
    /// opened; a setter item's prompt once the set revealed, its reference an `ExactKey` over the set's
    /// key commitment (a setter's item is generated: its kind is ExactMatch, scored at the keys' reveal).
    /// A suite item's prompt is not on chain: its claim discloses it (spec 17 §17.8.1), so this view never does.
    fn improvement_eval_item_view_v1(&self, line_id: &Hash64, epoch: u64, item: u32) -> Option<PalwImprovementEvalItemViewV1> {
        let row = self.improvement_item(line_id, epoch, item)?;
        let view = match row.source {
            PalwItemSourceV1::HoldOut => {
                let case = self.improvement_case(line_id, &row.case_id)?;
                let reference_ids = match case.case.reference {
                    PalwCaseReferenceV1::Continuation { .. } => case.revealed.clone(),
                    _ => None,
                };
                PalwImprovementEvalItemViewV1 {
                    item,
                    prompt_ids: Some(case.case.prompt_ids.clone()),
                    reference: case.case.reference,
                    reference_ids,
                    domain: case.case.domain,
                    dropped: row.dropped,
                }
            }
            PalwItemSourceV1::Setter { set_id, index } => {
                let set = self.improvement_setter_set(line_id, &set_id)?;
                PalwImprovementEvalItemViewV1 {
                    item,
                    prompt_ids: self.improvement_setter_prompt(line_id, &set_id, index).map(<[u32]>::to_vec),
                    reference: PalwCaseReferenceV1::ExactKey { commitment: set.commitment.keys_commitment },
                    reference_ids: None,
                    domain: 0,
                    dropped: row.dropped,
                }
            }
            // A suite item's prompt is not on chain either: its claim discloses the entry (the reference and an inclusion
            // proof under the registered dataset's `content_root`, spec 17 §17.8.1), so a node that holds the dataset's
            // content could evaluate it. This view discloses none: a node's loop plans no suite item yet.
            PalwItemSourceV1::Regression { .. } | PalwItemSourceV1::Safety { .. } => return None,
        };
        Some(view)
    }

    /// **Every open epoch's evaluation view** (the node lane's read door): for each open epoch past its
    /// draw, the drawn items with their disclosed prompts and references and every job that holds a
    /// claim. Bounded by `max_open_epochs` epochs of at most `max_items_per_epoch` items.
    pub fn improvement_eval_views_v1(&self) -> Vec<PalwImprovementEvalViewV1> {
        let mut out = Vec::new();
        for line in self.improvement_lines_iter_v1() {
            let Some(epoch) = line.open_epoch else { continue };
            let Some(header) = self.improvement_epoch(&line.line_id, epoch) else { continue };
            if !matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing) {
                continue;
            }
            let items: Vec<PalwImprovementEvalItemViewV1> =
                (0..header.items).filter_map(|i| self.improvement_eval_item_view_v1(&line.line_id, epoch, i)).collect();
            let jobs = self
                .improvement_eval_jobs_of_epoch(&line.line_id, epoch)
                .into_iter()
                .map(|(key, row)| {
                    let claim = row.claim.and_then(|c| {
                        let state = self.claim(&c.claim_id)?;
                        Some(PalwImprovementEvalClaimViewV1 {
                            claim_id: c.claim_id,
                            bond: c.executor,
                            accepted_daa: c.accepted_daa,
                            final_daa: c.final_daa,
                            voided: matches!(state.phase, PalwClaimPhaseV2::Voided { .. }),
                            trace_root: state.trace_root,
                            output_root: state.output_root,
                            execution_root: state.execution_root,
                            work_leaves: state.work_leaves,
                            score: c.score,
                        })
                    });
                    PalwImprovementEvalJobViewV1 { key, job: row.job.clone(), claim }
                })
                .collect();
            out.push(PalwImprovementEvalViewV1 { line_id: line.line_id, epoch, items, jobs });
        }
        out
    }

    /// **The improvement protocol's status** (the node lane's second door): every line the state holds,
    /// its usage, pool, head history, and every epoch row still kept with its candidates and grants.
    pub fn improvement_status_v1(&self) -> PalwImprovementStatusV1 {
        let mut lines = Vec::new();
        for line in self.improvement_lines_iter_v1() {
            let id = line.line_id;
            let mut epochs: Vec<PalwImprovementEpochStatusV1> = Vec::new();
            for e in 1..line.next_epoch {
                let Some(epoch) = self.improvement_epoch(&id, e) else { continue };
                epochs.push(PalwImprovementEpochStatusV1 {
                    epoch: epoch.clone(),
                    candidates: self.improvement_candidates(&id, e).into_iter().map(|(_, c)| c.clone()).collect(),
                    grants: self.improvement_grants(&id, e).into_iter().map(|(_, g)| g.clone()).collect(),
                });
            }
            lines.push(PalwImprovementLineStatusV1 {
                line: line.clone(),
                usage: self.improvement_usage(&id),
                pool: self.improvement_pool(&id),
                heads: self.improvement_head_history(&id),
                epochs,
            });
        }
        PalwImprovementStatusV1 {
            lines,
            composite_classes: self.improvement_composite_classes_v1().map(|(class, r)| (*class, *r)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_state_v1::test_rows;

    /// **The status door reads every line and the epoch rows the chain keeps; the evaluation view reads
    /// only open epochs past their draw.** A state carrying one governed line with its epoch in
    /// `Submission` reports the line, its header, usage, pool, head history and the epoch with its
    /// candidate; the evaluation view is empty until the epoch evaluates; and an empty state reports
    /// nothing at all (dormant below the fence).
    #[test]
    fn the_status_door_reads_the_lines_and_the_eval_view_waits_for_the_draw() {
        let empty = PalwChainStateV2::genesis();
        assert!(empty.improvement_status_v1().lines.is_empty());
        assert!(empty.improvement_eval_views_v1().is_empty());
        let mut state = PalwChainStateV2::genesis();
        test_rows::populate_v1(&mut state, 1);
        let status = state.improvement_status_v1();
        assert_eq!(status.lines.len(), 1);
        let line = &status.lines[0];
        assert_eq!(line.line.open_epoch, Some(1));
        assert!(line.usage.is_some() && line.pool.is_some());
        assert_eq!(line.heads.len(), 1);
        assert_eq!(line.epochs.len(), 1, "epoch 1 is the only epoch the header's next_epoch (2) leaves");
        assert_eq!(line.epochs[0].epoch.epoch, 1);
        assert_eq!(line.epochs[0].candidates.len(), 1);
        assert_eq!(line.epochs[0].grants.len(), 1);
        assert!(state.improvement_eval_views_v1().is_empty(), "a Submission epoch has drawn nothing: no evaluation view");
    }
}
