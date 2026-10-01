//! **RFC-0004 / spec 17: the Model Improvement Protocol in the fold.** A child module of
//! `palw_state_v2`, so it reads the builder and the state's tables directly and writes them only
//! through their one writers.
//!
//! * readers on [`PalwChainStateV2`] — what the lanes, the node and the RPC ask;
//! * the builder's helpers the lanes call from their object arms (candidates, material, setter sets,
//!   holds, scores, evaluation fees, S1) — every §17.7 / §17.6 / §17.11 check lives here once;
//! * the step-2 sweep ([`advance_improvement_v1`]): the bounded retirement of decided epochs' detail
//!   rows, every due line's transitions (§17.5), vesting, and the bounded flush of the earnings
//!   ledger into payouts;
//! * the core lane's object arms: the policy object (tag 70) and the rollback (tag 81);
//! * the hooks spec 15's arms call: usage at `Final` (§17.4.5), φ on the owner's leg (§17.11.1), and the
//!   refusal of a developer's promotion on a governed line (§17.4.1).

use super::*;
use crate::palw_improve_epoch_v1::*;
use crate::palw_improve_policy_v1::*;
use crate::palw_improve_promotion_v1::*;
use crate::palw_improve_state_v1::*;

/// Detail rows (pool entries, items, results) the retirement sweep deletes per block, at most
/// (spec 17 §17.5.4).
pub const PALW_IMPROVE_RETIRE_ROWS_PER_BLOCK_V1: usize = 512;
/// Earnings the flush turns into payout rows per block, at most (spec 17 §17.11.5).
pub const PALW_IMPROVE_PAYOUTS_PER_BLOCK_V1: usize = 2;
/// The flush leaves at least this many free rows in the pending-payout queue for the rest of the chain.
pub const PALW_IMPROVE_PAYOUT_QUEUE_RESERVE_V1: usize = 512;
/// The prefix byte of an improvement payout's key (claims: random; 0xFF model; 0xFE panel; 0x00 vesting).
pub const PALW_STATE_V2_IMPROVE_PAYOUT_KEY_PREFIX: u8 = 0xFD;
/// The most transitions one line takes in one block (a DAA jump across a whole epoch is 7).
const PALW_IMPROVE_STEPS_PER_ADVANCE_V1: usize = 16;

/// Where an admitted material item went (spec 17 §17.6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMaterialPlacementV1 {
    /// The open epoch's training material.
    EpochMaterial { epoch: u64 },
    /// The next epoch's training material (`pending_material`).
    NextEpochMaterial { epoch: u64 },
    /// The open epoch's evaluation pool (a hard case in `HoldOut`).
    HoldOut { epoch: u64 },
}

/// A pool entry's kind in its key [D13]: a hold-out case, or a setter set.
const POOL_KIND_HOLDOUT: u8 = 1;
const POOL_KIND_SETTER: u8 = 2;

/// Every pool entry of one epoch, in key order.
fn pool_range(line_id: &Hash64, epoch: u64) -> std::ops::RangeInclusive<(Hash64, u64, u8, Hash64)> {
    (*line_id, epoch, 0, Hash64::default())..=(*line_id, epoch, u8::MAX, Hash64::from_bytes([0xFF; 64]))
}

fn refused(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::ImprovementRefused(why)
}

// ---- readers -------------------------------------------------------------------------------

impl PalwChainStateV2 {
    pub fn improvement_line(&self, line_id: &Hash64) -> Option<&PalwImprovementLineV1> {
        self.improvement_lines.get(line_id)
    }

    /// **Every line the state holds** — governed, opting out or dissolved — in line-id order (the node
    /// lane's status door, `palw_improve_node_v1`).
    pub fn improvement_lines_iter_v1(&self) -> impl Iterator<Item = &PalwImprovementLineV1> {
        self.improvement_lines.values()
    }

    /// Is the line governed at `daa` (opted in, not yet out)?
    pub fn improvement_governed_at(&self, line_id: &Hash64, daa: u64) -> bool {
        self.improvement_lines.get(line_id).is_some_and(|line| line.governed_at(daa))
    }

    pub fn improvement_policy(&self, line_id: &Hash64) -> Option<&PalwImprovementPolicyV1> {
        self.improvement_policies.get(line_id).map(|record| &record.policy)
    }

    pub fn improvement_policy_record(&self, line_id: &Hash64) -> Option<&PalwImprovementPolicyRecordV1> {
        self.improvement_policies.get(line_id)
    }

    pub fn improvement_usage(&self, line_id: &Hash64) -> Option<PalwImprovementUsageV1> {
        self.improvement_usage.get(line_id).copied()
    }

    pub fn improvement_pool(&self, line_id: &Hash64) -> Option<PalwImprovementPoolV1> {
        self.improvement_pools.get(line_id).copied()
    }

    /// The line's kept head history, oldest first, with each entry's sequence number.
    pub fn improvement_head_history(&self, line_id: &Hash64) -> Vec<(u32, PalwLineageHeadEntryV1)> {
        self.improvement_heads.range((*line_id, 0)..=(*line_id, u32::MAX)).map(|((_, seq), entry)| (*seq, *entry)).collect()
    }

    /// The line's last head entry.
    pub fn improvement_last_head(&self, line_id: &Hash64) -> Option<PalwLineageHeadEntryV1> {
        self.improvement_heads.range((*line_id, 0)..=(*line_id, u32::MAX)).next_back().map(|(_, entry)| *entry)
    }

    pub fn improvement_epoch(&self, line_id: &Hash64, epoch: u64) -> Option<&PalwImprovementEpochV1> {
        self.improvement_epochs.get(&(*line_id, epoch))
    }

    /// The material of epoch `epoch` of the line — the next epoch's before it opens.
    pub fn improvement_material(&self, line_id: &Hash64, epoch: u64) -> Option<&PalwMaterialFrontierV1> {
        self.improvement_material.get(&(*line_id, epoch))
    }

    pub fn improvement_candidates(&self, line_id: &Hash64, epoch: u64) -> Vec<(u32, &PalwEpochCandidateV1)> {
        self.improvement_candidates
            .range((*line_id, epoch, 0)..=(*line_id, epoch, u32::MAX))
            .map(|((_, _, i), row)| (*i, row))
            .collect()
    }

    pub fn improvement_candidate(&self, line_id: &Hash64, epoch: u64, class_id: &Hash64) -> Option<(u32, &PalwEpochCandidateV1)> {
        self.improvement_candidates(line_id, epoch).into_iter().find(|(_, row)| row.class_id == *class_id)
    }

    pub fn improvement_pool_entries(&self, line_id: &Hash64, epoch: u64) -> Vec<&PalwPoolEntryV2> {
        self.improvement_pool_entries.range(pool_range(line_id, epoch)).map(|(_, row)| row).collect()
    }

    pub fn improvement_items(&self, line_id: &Hash64, epoch: u64) -> Vec<&PalwEvalItemV1> {
        self.improvement_items.range((*line_id, epoch, 0)..=(*line_id, epoch, u32::MAX)).map(|(_, row)| row).collect()
    }

    pub fn improvement_item(&self, line_id: &Hash64, epoch: u64, item: u32) -> Option<&PalwEvalItemV1> {
        self.improvement_items.get(&(*line_id, epoch, item))
    }

    pub fn improvement_result(
        &self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        subject: &PalwEvalSubjectV1,
    ) -> Option<&PalwEvalResultV1> {
        self.improvement_results.get(&(*line_id, epoch, item, *subject))
    }

    pub fn improvement_grants(&self, line_id: &Hash64, epoch: u64) -> Vec<(u32, &PalwRewardGrantV1)> {
        self.improvement_grants.range((*line_id, epoch, 0)..=(*line_id, epoch, u32::MAX)).map(|((_, _, i), row)| (*i, row)).collect()
    }

    /// What the earnings ledger owes `bond`, not yet flushed into a payout (spec 17 §17.11.5).
    pub fn improvement_earnings(&self, bond: &PalwBondKeyV2) -> u64 {
        self.improvement_earnings.get(bond).copied().unwrap_or(0)
    }

    /// The lines `class_id` heads.
    pub fn improvement_lines_headed_by(&self, class_id: &Hash64) -> Vec<Hash64> {
        self.improvement_heads_of.get(class_id).map(|lines| lines.iter().copied().collect()).unwrap_or_default()
    }

    /// Epochs open network-wide (the `max_open_epochs` ceiling).
    pub fn improvement_open_epoch_count(&self) -> u32 {
        self.improvement_open_epochs
    }

    /// Result rows reserved network-wide (the `max_live_results` ceiling).
    pub fn improvement_live_results(&self) -> u64 {
        self.improvement_live_results
    }

    /// Lines governed at `daa`, network-wide (the `max_governed_lines` ceiling).
    pub fn improvement_governed_count(&self, daa: u64) -> usize {
        self.improvement_lines.values().filter(|line| line.governed_at(daa)).count()
    }

    /// **Every open epoch, as a node reads it** (the read door): the line, its policy, the epoch's
    /// header, its candidates and its items. Bounded by `max_open_epochs`.
    pub fn improvement_open_epoch_views_v1(&self) -> Vec<PalwImprovementEpochViewV1> {
        self.improvement_lines
            .values()
            .filter_map(|line| {
                let epoch = self.improvement_epoch(&line.line_id, line.open_epoch?)?.clone();
                Some(PalwImprovementEpochViewV1 {
                    line: line.clone(),
                    policy: self.improvement_policy(&line.line_id)?.clone(),
                    candidates: self
                        .improvement_candidates(&line.line_id, epoch.epoch)
                        .into_iter()
                        .map(|(_, row)| row.clone())
                        .collect(),
                    items: self.improvement_items(&line.line_id, epoch.epoch).into_iter().copied().collect(),
                    epoch,
                })
            })
            .collect()
    }

    /// **The subjects of an epoch** (spec 17 §17.8.2): the parent, every candidate in acceptance order,
    /// and the regression check's predecessor.
    pub fn improvement_subjects(&self, line_id: &Hash64, epoch: u64) -> Vec<PalwEvalSubjectV1> {
        let Some(header) = self.improvement_epoch(line_id, epoch) else { return Vec::new() };
        let mut subjects = vec![PalwEvalSubjectV1::Parent];
        subjects.extend(
            self.improvement_candidates(line_id, epoch).into_iter().map(|(_, row)| PalwEvalSubjectV1::Candidate(row.class_id)),
        );
        if let Some(previous) = header.previous {
            subjects.push(PalwEvalSubjectV1::Previous(previous));
        }
        subjects
    }
}

/// The recorded scores of one epoch, as the promotion rule reads them.
struct EpochScores<'s> {
    state: &'s PalwChainStateV2,
    line_id: Hash64,
    epoch: u64,
}

impl PalwImproveScoresV1 for EpochScores<'_> {
    fn score(&self, item: u32, subject: &PalwEvalSubjectV1, kind: PalwScoringKindV1) -> Option<i64> {
        self.state
            .improvement_result(&self.line_id, self.epoch, item, subject)
            .and_then(|result| result.scores.iter().find(|score| score.kind == kind))
            .map(|score| score.value)
    }

    fn any_score(&self, item: u32, kind: PalwScoringKindV1) -> bool {
        let lo = (self.line_id, self.epoch, item, PalwEvalSubjectV1::Parent);
        let hi = (self.line_id, self.epoch, item, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])));
        self.state.improvement_results.range(lo..=hi).any(|(_, result)| result.scores.iter().any(|score| score.kind == kind))
    }
}

/// **The evaluation lane's completion hook** (spec 17 §17.5.3 step 7): is any evaluation claim of the
/// epoch not yet final? The evaluation lane (A6) replaces this body with its job table's answer; until
/// then every epoch scores at `t_score`.
fn palw_improve_eval_pending_hook_v1(state: &PalwChainStateV2, line_id: &Hash64, epoch: u64) -> bool {
    state.improvement_eval_pending_v1(line_id, epoch)
}

/// **The material lane's reveal hook** (spec 17 §17.5.3 step 7): does a drawn setter set or hold-out
/// case still owe a reveal (prompts, a key or a reference)? The material lane's (A4) answer.
fn palw_improve_material_pending_hook_v1(state: &PalwChainStateV2, line_id: &Hash64, epoch: u64) -> bool {
    super::palw_improve_material_fold_v1::palw_improve_material_pending_v1(state, line_id, epoch)
}

/// **The material lane's settlement before scoring** (spec 17 §17.9.1): drop every drawn item whose
/// prompts, key or reference was never revealed (`drop_improvement_item_v1`) and forfeit the
/// non-revealing setters' holds — the material lane's (A4).
fn palw_improve_material_before_scoring_hook_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    epoch: u64,
) -> Result<(), PalwStateV2Error> {
    super::palw_improve_material_fold_v1::palw_improve_material_before_scoring_v1(builder, line_id, epoch)
}

/// **The material lane's dataset reader** (spec 17 §17.11.3): a registered dataset's registrant — the
/// material lane's (A4) dataset table.
fn palw_improve_dataset_registrant_hook_v1(state: &PalwChainStateV2, line_id: &Hash64, dataset_id: &Hash64) -> Option<PalwBondKeyV2> {
    state.improvement_dataset(line_id, dataset_id).map(|record| record.registrant)
}

/// **The evaluation lane's claim predicate** (spec 17 §17.4.5): is the claim an evaluation claim (which
/// never counts toward usage)? The evaluation lane (A6) replaces this body with
/// `palw_fp_claim_is_evaluation_v1`; until then no claim is one.
fn palw_improve_claim_is_evaluation_hook_v1(claim: &PalwClaimStateV2) -> bool {
    super::palw_improve_eval_fold_v1::palw_improve_claim_is_evaluation_v1(claim)
}

// ---- the builder's helpers -----------------------------------------------------------------

// Several of these are the lanes' API (A4–A6: candidates, material, setter sets, scores, fees, S1),
// called from their arms once those land.
#[allow(dead_code)]
impl TransitionBuilder<'_> {
    fn improvement_active_v1(&self, daa: u64) -> Result<(), PalwStateV2Error> {
        if self.params.improve_active_at(daa) { Ok(()) } else { Err(refused("palw_improvement_v1 is not in force")) }
    }

    fn improvement_ceilings_v1(&self) -> Result<crate::palw_improve_v1::PalwImprovementCeilingsV1, PalwStateV2Error> {
        self.params.improve_ceilings().ok_or_else(|| refused("the network states no improvement ceilings"))
    }

    fn improvement_line_row_v1(&self, line_id: &Hash64) -> Result<PalwImprovementLineV1, PalwStateV2Error> {
        self.state.improvement_lines.get(line_id).cloned().ok_or_else(|| refused("the line is not governed"))
    }

    fn improvement_policy_of_v1(&self, line_id: &Hash64) -> Result<PalwImprovementPolicyV1, PalwStateV2Error> {
        self.state
            .improvement_policies
            .get(line_id)
            .map(|record| record.policy.clone())
            .ok_or_else(|| refused("the line has no policy"))
    }

    fn improvement_pool_of_v1(&self, line_id: &Hash64) -> PalwImprovementPoolV1 {
        self.state.improvement_pools.get(line_id).copied().unwrap_or_default()
    }

    fn improvement_epoch_of_v1(&self, line_id: &Hash64, epoch: u64) -> Result<PalwImprovementEpochV1, PalwStateV2Error> {
        self.state.improvement_epochs.get(&(*line_id, epoch)).cloned().ok_or_else(|| refused("no such epoch"))
    }

    /// The line's open epoch, its header and its policy.
    fn improvement_open_epoch_v1(
        &self,
        line_id: &Hash64,
    ) -> Result<(PalwImprovementEpochV1, PalwImprovementPolicyV1), PalwStateV2Error> {
        let line = self.improvement_line_row_v1(line_id)?;
        let epoch = line.open_epoch.ok_or_else(|| refused("the line has no open epoch"))?;
        Ok((self.improvement_epoch_of_v1(line_id, epoch)?, self.improvement_policy_of_v1(line_id)?))
    }

    // ---- money (spec 17 §17.11) ----

    /// A bond's collateral free of every R-core+ commitment, at `daa`.
    fn improvement_free_collateral_v1(&self, bond: &PalwBondKeyV2, daa: u64) -> Result<u128, PalwStateV2Error> {
        let record = self.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(refused("the paying bond is not Active"));
        }
        Ok((record.collateral as u128)
            .saturating_sub(self.committed_at(bond, daa))
            .saturating_sub(self.read().accuser_ledger_v1(bond, daa)))
    }

    /// **Debit a bond** (§17.11.1): the amount leaves its collateral as a slash does (destroyed there),
    /// refused unless its free collateral covers it.
    fn debit_improvement_bond_v1(&mut self, bond: &PalwBondKeyV2, amount: u64, daa: u64) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        if self.improvement_free_collateral_v1(bond, daa)? < amount as u128 {
            return Err(refused("the bond's free collateral does not cover the payment"));
        }
        let debited = self.slash_bond(*bond, amount as u128)?;
        if debited != amount {
            return Err(refused("the bond could not be debited in full"));
        }
        Ok(())
    }

    /// Credit the earnings ledger (§17.11.5): what the flush pays out later.
    fn credit_improvement_earnings_v1(&mut self, bond: &PalwBondKeyV2, amount: u64) {
        if amount == 0 {
            return;
        }
        let owed = self.state.improvement_earnings.get(bond).copied().unwrap_or(0).saturating_add(amount);
        self.write_improvement_earning(*bond, Some(owed));
    }

    fn write_improvement_pool_row_v1(&mut self, line_id: &Hash64, pool: PalwImprovementPoolV1) {
        self.write_improvement_pool(*line_id, Some(pool));
    }

    /// **A hold** (§17.11.1): debit `bond` into the pool's `held`.
    pub(crate) fn debit_improvement_hold_v1(
        &mut self,
        line_id: &Hash64,
        bond: &PalwBondKeyV2,
        amount: u64,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.improvement_line_row_v1(line_id)?;
        self.debit_improvement_bond_v1(bond, amount, daa)?;
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.held = pool.held.checked_add(amount).ok_or(PalwStateV2Error::Overflow("improvement held"))?;
        pool.held_in += amount as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        Ok(())
    }

    /// **A fee** (§17.11.1): debit `bond` into the pool's balance.
    fn debit_improvement_fee_v1(
        &mut self,
        line_id: &Hash64,
        bond: &PalwBondKeyV2,
        amount: u64,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.debit_improvement_bond_v1(bond, amount, daa)?;
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.balance = pool.balance.checked_add(amount).ok_or(PalwStateV2Error::Overflow("improvement balance"))?;
        pool.fees_in += amount as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        Ok(())
    }

    /// **A refund of a hold** to `recipient` (§17.11.1): held → the earnings ledger.
    pub(crate) fn release_improvement_hold_v1(
        &mut self,
        line_id: &Hash64,
        recipient: &PalwBondKeyV2,
        amount: u64,
    ) -> Result<(), PalwStateV2Error> {
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.held = pool.held.checked_sub(amount).ok_or(PalwStateV2Error::Overflow("improvement held underflow"))?;
        pool.refunded += amount as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        self.credit_improvement_earnings_v1(recipient, amount);
        Ok(())
    }

    /// **A forfeit of a hold** (§17.11.1): held → the balance.
    pub(crate) fn forfeit_improvement_hold_v1(&mut self, line_id: &Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.held = pool.held.checked_sub(amount).ok_or(PalwStateV2Error::Overflow("improvement held underflow"))?;
        pool.balance = pool.balance.checked_add(amount).ok_or(PalwStateV2Error::Overflow("improvement balance"))?;
        pool.forfeited_in += amount as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        Ok(())
    }

    // ---- candidates (spec 17 §17.7) ----

    /// **May `submitter` enter `class_id` in the line's epoch now?** Every §17.7 check but the payment.
    pub(crate) fn improvement_candidate_admissible_v1(
        &self,
        line_id: &Hash64,
        epoch: u64,
        submitter: &PalwBondKeyV2,
        class_id: &Hash64,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.improvement_active_v1(daa)?;
        let line = self.improvement_line_row_v1(line_id)?;
        if line.open_epoch != Some(epoch) {
            return Err(refused("a candidate for an epoch that is not the line's open one"));
        }
        let header = self.improvement_epoch_of_v1(line_id, epoch)?;
        if header.state != PalwEpochStateV1::Submission || daa < header.times.t_fix || daa >= header.times.t_close {
            return Err(refused("a candidate outside [t_fix, t_close)"));
        }
        let policy = self.improvement_policy_of_v1(line_id)?;
        if header.candidates >= policy.k_max as u32 {
            return Err(refused("the epoch already holds k_max candidates"));
        }
        if *class_id == line.head {
            return Err(refused("the head is not a candidate"));
        }
        if self.state.improvement_candidate(line_id, epoch, class_id).is_some() {
            return Err(refused("the class is already a candidate of this epoch"));
        }
        if line.is_barred(submitter, daa) {
            return Err(refused("the submitter is barred after a rollback"));
        }
        Ok(())
    }

    /// **Admit a candidate** (spec 17 §17.7): the checks, the payment (registration fee, bond, escrow —
    /// all held until the epoch decides), the row. Returns its index in acceptance order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_improvement_candidate_v1(
        &mut self,
        line_id: &Hash64,
        epoch: u64,
        class_id: &Hash64,
        submitter: &PalwBondKeyV2,
        artifact: crate::palw_improve_artifact_v1::PalwTirArtifactRefV1,
        declarations_digest: Hash64,
        datasets: Vec<(Hash64, u16)>,
        daa: u64,
    ) -> Result<u32, PalwStateV2Error> {
        self.improvement_candidate_admissible_v1(line_id, epoch, submitter, class_id, daa)?;
        let policy = self.improvement_policy_of_v1(line_id)?;
        let fee = policy.fees.registration_fee;
        let bond = policy.fees.candidate_bond;
        let escrow = palw_improvement_jobs_per_subject_v1(&policy.eval, true)
            .checked_mul(policy.fees.eval_fee_per_job)
            .ok_or(PalwStateV2Error::Overflow("candidate escrow"))?;
        let held = bond.checked_add(escrow).ok_or(PalwStateV2Error::Overflow("candidate payment"))?;
        let total = held.checked_add(fee).ok_or(PalwStateV2Error::Overflow("candidate payment"))?;
        // [D2/E21] One debit; the registration fee enters the balance now (the draw's escrow may use
        // it), the bond and the candidate's own escrow are held until the epoch ends.
        self.debit_improvement_bond_v1(submitter, total, daa)?;
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.balance = pool.balance.checked_add(fee).ok_or(PalwStateV2Error::Overflow("improvement balance"))?;
        pool.fees_in += fee as u128;
        pool.held = pool.held.checked_add(held).ok_or(PalwStateV2Error::Overflow("improvement held"))?;
        pool.held_in += held as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        let mut header = self.improvement_epoch_of_v1(line_id, epoch)?;
        let index = header.candidates;
        self.write_improvement_candidate(
            (*line_id, epoch, index),
            Some(PalwEpochCandidateV1 {
                class_id: *class_id,
                submitter: *submitter,
                artifact,
                declarations_digest,
                datasets,
                fee_paid: fee,
                bond,
                escrow,
                escrow_spent: 0,
                submitted_daa: daa,
                counts: None,
            }),
        );
        header.candidates += 1;
        self.write_improvement_epoch((*line_id, epoch), Some(header));
        Ok(index)
    }

    // ---- material (spec 17 §17.6) ----

    /// **Admit a material item** (spec 17 §17.6.1): a hard case pays `hard_case_fee` into the balance
    /// and joins the epoch's material (`Open`), its evaluation pool (`HoldOut`), or the next epoch's
    /// material; a dataset or a teaching artifact joins the epoch's material when `Open`, the next
    /// epoch's otherwise.
    pub(crate) fn note_improvement_material_v1(
        &mut self,
        line_id: &Hash64,
        kind: PalwMaterialKindV1,
        id: &Hash64,
        supplier: &PalwBondKeyV2,
        daa: u64,
    ) -> Result<PalwMaterialPlacementV1, PalwStateV2Error> {
        self.improvement_active_v1(daa)?;
        let line = self.improvement_line_row_v1(line_id)?;
        if !line.governed_at(daa) {
            return Err(refused("the line is not governed"));
        }
        let policy = self.improvement_policy_of_v1(line_id)?;
        let open = line.open_epoch.map(|e| self.improvement_epoch_of_v1(line_id, e)).transpose()?;
        let into_holdout = open.as_ref().is_some_and(|h| h.state == PalwEpochStateV1::HoldOut) && kind == PalwMaterialKindV1::HardCase;
        if into_holdout
            && open.as_ref().is_some_and(|h| h.holdout_cases >= policy.eval.n.saturating_mul(PALW_IMPROVE_HOLDOUT_FACTOR_V1))
        {
            return Err(refused("the epoch's hold-out pool is full (4·n)"));
        }
        // [D13] An id enters an epoch's pool once.
        if into_holdout
            && let Some(header) = &open
            && self.state.improvement_pool_entries.contains_key(&(*line_id, header.epoch, POOL_KIND_HOLDOUT, *id))
        {
            return Err(refused("the case is already in the epoch's hold-out pool"));
        }
        if kind == PalwMaterialKindV1::HardCase {
            self.debit_improvement_fee_v1(line_id, supplier, policy.fees.hard_case_fee, daa)?;
        }
        if let Some(header) = &open
            && into_holdout
        {
            let mut header = header.clone();
            self.write_improvement_pool_entry(
                (*line_id, header.epoch, POOL_KIND_HOLDOUT, *id),
                Some(PalwPoolEntryV2::HoldOut { id: *id, supplier: *supplier }),
            );
            header.pool_entries += 1;
            header.holdout_cases += 1;
            let epoch = header.epoch;
            self.write_improvement_epoch((*line_id, epoch), Some(header));
            return Ok(PalwMaterialPlacementV1::HoldOut { epoch });
        }
        let (epoch, placement) = match &open {
            Some(header) if header.state == PalwEpochStateV1::Open => {
                (header.epoch, PalwMaterialPlacementV1::EpochMaterial { epoch: header.epoch })
            }
            _ => (line.next_epoch, PalwMaterialPlacementV1::NextEpochMaterial { epoch: line.next_epoch }),
        };
        let mut frontier = self.state.improvement_material.get(&(*line_id, epoch)).cloned().unwrap_or_default();
        palw_improve_material_append_v1(&mut frontier, palw_improve_material_leaf_v1(kind, id));
        self.write_improvement_material((*line_id, epoch), Some(frontier));
        Ok(placement)
    }

    /// **Enter a setter set** (spec 17 §17.6.2) before `t_close`, paying `setter_bond` as a hold.
    pub(crate) fn add_improvement_setter_set_v1(
        &mut self,
        line_id: &Hash64,
        set_id: &Hash64,
        setter: &PalwBondKeyV2,
        items: u32,
        daa: u64,
    ) -> Result<u64, PalwStateV2Error> {
        self.improvement_active_v1(daa)?;
        let (mut header, policy) = self.improvement_open_epoch_v1(line_id)?;
        if !matches!(header.state, PalwEpochStateV1::Open | PalwEpochStateV1::Submission) || daa >= header.times.t_close {
            return Err(refused("a setter set after t_close"));
        }
        if header.setter_sets >= PALW_IMPROVE_MAX_SETTER_SETS_V1 {
            return Err(refused("the epoch already holds 16 setter sets"));
        }
        if items == 0 || items > policy.eval.n {
            return Err(refused("a setter set holds 1 to n items"));
        }
        // [D13] A set enters an epoch's pool once.
        if self.state.improvement_pool_entries.contains_key(&(*line_id, header.epoch, POOL_KIND_SETTER, *set_id)) {
            return Err(refused("the set is already in the epoch's pool"));
        }
        self.debit_improvement_hold_v1(line_id, setter, policy.fees.setter_bond, daa)?;
        self.write_improvement_pool_entry(
            (*line_id, header.epoch, POOL_KIND_SETTER, *set_id),
            Some(PalwPoolEntryV2::SetterSet { set_id: *set_id, setter: *setter, items }),
        );
        header.pool_entries += 1;
        header.setter_sets += 1;
        let epoch = header.epoch;
        self.write_improvement_epoch((*line_id, epoch), Some(header));
        Ok(epoch)
    }

    /// **Drop an item for every subject** (spec 17 §17.9.1): a setter that never revealed.
    pub(crate) fn drop_improvement_item_v1(&mut self, line_id: &Hash64, epoch: u64, item: u32) -> Result<(), PalwStateV2Error> {
        let mut row = self.state.improvement_items.get(&(*line_id, epoch, item)).copied().ok_or_else(|| refused("no such item"))?;
        row.dropped = true;
        self.write_improvement_item((*line_id, epoch, item), Some(row));
        Ok(())
    }

    // ---- evaluation (spec 17 §17.8) ----

    /// **Record a final score** (spec 17 §17.8.3). A second score of the same kind for the same
    /// `(item, subject)` is refused.
    pub(crate) fn record_improvement_score_v1(
        &mut self,
        line_id: &Hash64,
        epoch: u64,
        item: u32,
        subject: PalwEvalSubjectV1,
        score: PalwEvalScoreV1,
    ) -> Result<(), PalwStateV2Error> {
        let header = self.improvement_epoch_of_v1(line_id, epoch)?;
        if !matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing) {
            return Err(refused("a score outside the epoch's evaluation"));
        }
        if item >= header.items {
            return Err(refused("a score for an item the epoch did not draw"));
        }
        if !self.state.improvement_subjects(line_id, epoch).contains(&subject) {
            return Err(refused("a score for a subject the epoch does not evaluate"));
        }
        // [D6] A score outside its kind's range is refused: ExactMatch 0 or 1, Pairwise −1, 0 or 1, Judge
        // inside the policy's `[lo, hi]`; the one outcome function then reads only legal values.
        let policy = self.improvement_policy_of_v1(line_id)?;
        let in_range = match score.kind {
            PalwScoringKindV1::ExactMatch => matches!(score.value, 0 | 1),
            PalwScoringKindV1::Pairwise => matches!(score.value, -1..=1),
            PalwScoringKindV1::Judge => policy.eval.stages.iter().any(|stage| {
                matches!(stage.params, PalwScoringParamsV1::Judge { lo, hi } if (lo as i64..=hi as i64).contains(&score.value))
            }),
            PalwScoringKindV1::RefLogLik => true,
        };
        if !in_range {
            return Err(refused("a score outside its kind's range"));
        }
        let key = (*line_id, epoch, item, subject);
        let mut row =
            self.state.improvement_results.get(&key).cloned().unwrap_or(PalwEvalResultV1 { item, subject, scores: Vec::new() });
        if row.scores.iter().any(|s| s.kind == score.kind) {
            return Err(refused("the item already has this subject's score of this kind"));
        }
        row.scores.push(score);
        self.write_improvement_result(key, Some(row));
        Ok(())
    }

    /// **Pay an evaluation job's fee** (spec 17 §17.11.2) to `executor` from the subject's escrow.
    /// Returns the fee paid (0 once the escrow is spent).
    pub(crate) fn pay_improvement_eval_fee_v1(
        &mut self,
        line_id: &Hash64,
        epoch: u64,
        subject: &PalwEvalSubjectV1,
        executor: &PalwBondKeyV2,
    ) -> Result<u64, PalwStateV2Error> {
        let policy = self.improvement_policy_of_v1(line_id)?;
        let fee = policy.fees.eval_fee_per_job;
        let mut header = self.improvement_epoch_of_v1(line_id, epoch)?;
        // [E23] Paid only while the epoch evaluates or closes: a claim final after the epoch's end is
        // not paid (the unspent escrow has gone back), and is not scored either.
        if !matches!(header.state, PalwEpochStateV1::Evaluating | PalwEpochStateV1::Closing) {
            return Ok(0);
        }
        let paid = match subject {
            PalwEvalSubjectV1::Parent => {
                let paid = fee.min(header.escrow.parent - header.escrow.parent_spent);
                header.escrow.parent_spent += paid;
                self.write_improvement_epoch((*line_id, epoch), Some(header));
                paid
            }
            PalwEvalSubjectV1::Previous(_) => {
                let paid = fee.min(header.escrow.previous - header.escrow.previous_spent);
                header.escrow.previous_spent += paid;
                self.write_improvement_epoch((*line_id, epoch), Some(header));
                paid
            }
            PalwEvalSubjectV1::Candidate(class_id) => {
                let (index, row) =
                    self.state.improvement_candidate(line_id, epoch, class_id).ok_or_else(|| refused("no such candidate"))?;
                let mut row = row.clone();
                let paid = fee.min(row.escrow - row.escrow_spent);
                row.escrow_spent += paid;
                self.write_improvement_candidate((*line_id, epoch, index), Some(row));
                paid
            }
        };
        if paid > 0 {
            let mut pool = self.improvement_pool_of_v1(line_id);
            pool.held = pool.held.checked_sub(paid).ok_or(PalwStateV2Error::Overflow("improvement held underflow"))?;
            pool.paid += paid as u128;
            self.write_improvement_pool_row_v1(line_id, pool);
            self.credit_improvement_earnings_v1(executor, paid);
        }
        Ok(paid)
    }

    // ---- S1 (spec 17 §17.11.3) ----

    /// **Pay an S1 event** at once while the period's budget lasts: `s1_bounty` for a matching
    /// `Answer`, `s1_setter_reward` for a problem the head fails. Returns the amount paid (0 once the
    /// budget is spent).
    pub(crate) fn grant_improvement_s1_v1(
        &mut self,
        line_id: &Hash64,
        recipient: &PalwBondKeyV2,
        stage: PalwRewardStageV1,
    ) -> Result<u64, PalwStateV2Error> {
        let policy = self.improvement_policy_of_v1(line_id)?;
        let amount = match stage {
            PalwRewardStageV1::S1Bounty => policy.fees.s1_bounty,
            PalwRewardStageV1::S1Setter => policy.fees.s1_setter_reward,
            _ => return Err(refused("S1 pays only bounties and setter rewards")),
        };
        let mut pool = self.improvement_pool_of_v1(line_id);
        if amount == 0 || pool.s1_budget < amount || pool.balance < amount {
            return Ok(0);
        }
        pool.s1_budget -= amount;
        pool.balance -= amount;
        pool.paid += amount as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        self.credit_improvement_earnings_v1(recipient, amount);
        Ok(amount)
    }

    // ---- hooks spec 15's arms call ----

    /// **Usage at `Final`** (spec 17 §17.4.5): each governed line the claim's class heads counts it.
    pub(super) fn note_improvement_usage_at_final_v1(&mut self, claim: &PalwClaimStateV2, final_daa: u64) {
        if !self.params.improve_active_at(final_daa) || palw_improve_claim_is_evaluation_hook_v1(claim) {
            return;
        }
        for line_id in self.state.improvement_lines_headed_by(&claim.class_id) {
            let Some(line) = self.state.improvement_lines.get(&line_id) else { continue };
            if !line.governed_at(final_daa) {
                continue;
            }
            let Some(policy) = self.state.improvement_policy(&line_id) else { continue };
            let measure = policy.usage.measure;
            let Some(mut usage) = self.state.improvement_usage.get(&line_id).copied() else { continue };
            if claim.accepted_daa < usage.since_daa {
                continue;
            }
            usage.usage = usage.usage.saturating_add(match measure {
                PalwUsageMeasureV1::Claims => 1,
                PalwUsageMeasureV1::WorkLeaves => claim.pwu as u128,
            });
            self.write_improvement_usage(line_id, Some(usage));
        }
    }

    /// **φ** (spec 17 §17.11.1): the pool's part of a governed line's owner leg. Returns what stays the
    /// owner's.
    pub(super) fn take_improvement_phi_v1(&mut self, line_id: &Hash64, to_owner: u64, daa: u64) -> u64 {
        if !self.params.improve_active_at(daa) || !self.state.improvement_governed_at(line_id, daa) {
            return to_owner;
        }
        let Some(phi) = self.state.improvement_policy(line_id).map(|p| p.phi_permille) else { return to_owner };
        let to_pool = ((to_owner as u128 * phi as u128) / 1000) as u64;
        if to_pool == 0 {
            return to_owner;
        }
        let mut pool = self.improvement_pool_of_v1(line_id);
        pool.balance = pool.balance.saturating_add(to_pool);
        pool.phi_in += to_pool as u128;
        self.write_improvement_pool_row_v1(line_id, pool);
        to_owner - to_pool
    }

    /// **The developer's promotion on a governed line** (spec 17 §17.4.1) is refused.
    pub(super) fn refuse_developer_promotion_v1(&self, line_id: &Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
        if self.params.improve_active_at(daa) && self.state.improvement_governed_at(line_id, daa) {
            return Err(PalwStateV2Error::ImprovementLineGoverned(*line_id));
        }
        Ok(())
    }
}

// ---- the policy object (tag 70, spec 17 §17.4.2) ----------------------------------------------

pub(super) fn apply_improvement_policy_set_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    payload: &PalwImprovementPolicySetV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let line_id = payload.line_id;
    let policy_refused = PalwStateV2Error::ImprovementPolicyRefused;
    let spec15 = builder.state.model_line_or_founding(&line_id).ok_or_else(|| policy_refused("no such line"))?;
    if !spec15.is_active() || spec15.owner.is_none() {
        return Err(policy_refused("the line is retired or has no owner"));
    }
    let ceilings = builder.improvement_ceilings_v1()?;
    let row = builder.state.improvement_lines.get(&line_id).cloned();
    let expected = row.as_ref().map_or(1, |r| r.policy_sequence + 1);
    if payload.sequence != expected {
        return Err(policy_refused("the policy object's sequence is not the line's next"));
    }
    if let Some(policy) = &payload.policy {
        palw_improvement_policy_check_v1(policy, &ceilings).map_err(policy_refused)?;
        if policy.eval.judge_set.iter().any(|judge| !builder.state.tir_classes.contains_key(judge)) {
            return Err(policy_refused("a judge is not an admitted IR class"));
        }
    }
    let digest = payload.policy.as_ref().map(palw_improvement_policy_digest_v1);
    let governed_row = row.filter(|r| r.status != PalwImprovementLineStatusV1::Dissolved);
    let dissolved_row =
        builder.state.improvement_lines.get(&line_id).filter(|r| r.status == PalwImprovementLineStatusV1::Dissolved).cloned();
    match (governed_row, &payload.policy) {
        // Opt in (§17.4.2); a dissolved line opts in again with its sequence continuing.
        (None, Some(policy)) => {
            let row = dissolved_row;
            if !builder.state.tir_classes.contains_key(&spec15.class_id) {
                return Err(policy_refused("only an IR line is governed"));
            }
            if builder.state.improvement_governed_count(daa) >= ceilings.max_governed_lines as usize {
                return Err(policy_refused("the network governs max_governed_lines lines already"));
            }
            let head_seq = row.as_ref().map_or(0, |r| r.head_seq);
            let next_epoch = row.as_ref().map_or(1, |r| r.next_epoch);
            let line = PalwImprovementLineV1 {
                line_id,
                class_id: spec15.class_id,
                policy_digest: digest.expect("Some policy"),
                policy_sequence: payload.sequence,
                status: PalwImprovementLineStatusV1::Governed,
                governed_from_daa: daa,
                head: spec15.class_id,
                head_seq: head_seq + 1,
                next_epoch,
                open_epoch: None,
                next_due_daa: palw_improve_next_boundary_v1(daa, policy.windows.grid),
                barred: row.map(|r| r.barred).unwrap_or_default(),
                last_promotion: None,
                regression_epoch: None,
                regression_check: None,
            };
            builder.write_improvement_policy(line_id, Some(PalwImprovementPolicyRecordV1 { policy: policy.clone(), pending: None }));
            builder.write_improvement_usage(line_id, Some(PalwImprovementUsageV1 { usage: 0, since_daa: daa }));
            builder.write_improvement_pool(line_id, Some(PalwImprovementPoolV1::default()));
            push_head_v1(
                builder,
                &line_id,
                head_seq,
                PalwLineageHeadEntryV1 { epoch: 0, class_id: spec15.class_id, previous: None, daa, cause: PalwHeadCauseV1::OptIn },
            );
            builder.write_improvement_line(line_id, Some(line));
        }
        (None, None) => return Err(policy_refused("an opt-out of a line that is not governed")),
        (Some(mut line), Some(policy)) => {
            // [D14] A policy before `effective_daa` cancels an opt-out; past it the line is no longer
            // governed and opts in again only once it has dissolved.
            if !line.governed_at(daa) {
                return Err(policy_refused("the line has opted out; it opts in again once it dissolves"));
            }
            line.policy_sequence = payload.sequence;
            line.status = PalwImprovementLineStatusV1::Governed;
            let mut record =
                builder.state.improvement_policies.get(&line_id).cloned().ok_or_else(|| policy_refused("the line has no policy"))?;
            let idle = line.open_epoch.is_none();
            if idle {
                record.policy = policy.clone();
                record.pending = None;
                line.policy_digest = digest.expect("Some policy");
            } else {
                record.pending = Some(Box::new(policy.clone()));
            }
            builder.write_improvement_policy(line_id, Some(record));
            if idle {
                // [E28] The next check lands on the new grid (or an earlier vesting step, §17.5.2).
                line.next_due_daa = next_due_v1(builder, &line, daa)?;
            }
            builder.write_improvement_line(line_id, Some(line));
        }
        (Some(line), None) if line.status != PalwImprovementLineStatusV1::Governed => {
            // [D14] One opt-out: a second would restart the delay (or re-govern a line past it).
            return Err(policy_refused("the line is already opting out"));
        }
        (Some(mut line), None) => {
            line.policy_sequence = payload.sequence;
            let grid = builder.improvement_policy_of_v1(&line_id)?.windows.grid;
            let effective_daa = line.open_epoch.is_none().then(|| daa.saturating_add(grid));
            line.status = PalwImprovementLineStatusV1::OptingOut { effective_daa };
            if let Some(at) = effective_daa {
                line.next_due_daa = line.next_due_daa.min(at);
            }
            builder.write_improvement_line(line_id, Some(line));
        }
    }
    Ok(())
}

/// Append a head entry at `seq`, dropping the one 64 behind it.
fn push_head_v1(builder: &mut TransitionBuilder<'_>, line_id: &Hash64, seq: u32, entry: PalwLineageHeadEntryV1) {
    builder.write_improvement_head((*line_id, seq), Some(entry));
    if seq >= PALW_IMPROVE_HEAD_HISTORY_MAX_V1 as u32 {
        builder.write_improvement_head((*line_id, seq - PALW_IMPROVE_HEAD_HISTORY_MAX_V1 as u32), None);
    }
}

/// Move the head (spec 17 §17.4.1): the entry, the line's head, and the usage restart.
fn move_head_v1(
    builder: &mut TransitionBuilder<'_>,
    line: &mut PalwImprovementLineV1,
    epoch: u64,
    to: Hash64,
    daa: u64,
    cause: PalwHeadCauseV1,
) {
    push_head_v1(
        builder,
        &line.line_id,
        line.head_seq,
        PalwLineageHeadEntryV1 { epoch, class_id: to, previous: Some(line.head), daa, cause },
    );
    line.head_seq += 1;
    line.head = to;
    builder.write_improvement_usage(line.line_id, Some(PalwImprovementUsageV1 { usage: 0, since_daa: daa }));
}

// ---- the sponsor's deposit (tag 82, spec 17 §17.11.1) --------------------------------------------

/// A deposit credits a governed line's pool; anything else is refused, and P-B1 pays it back.
pub(super) fn apply_improvement_pool_funded_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    payload: &PalwImprovementPoolFundingV1,
) -> Result<(), PalwStateV2Error> {
    if payload.amount == 0 {
        return Err(refused("a deposit of nothing"));
    }
    if !builder.state.improvement_governed_at(&payload.line_id, ctx.daa_score) {
        return Err(refused("a deposit to a line that is not governed"));
    }
    let mut pool = builder.improvement_pool_of_v1(&payload.line_id);
    pool.balance = pool.balance.checked_add(payload.amount).ok_or(PalwStateV2Error::Overflow("improvement balance"))?;
    pool.deposited += payload.amount as u128;
    builder.write_improvement_pool(payload.line_id, Some(pool));
    Ok(())
}

// ---- the rollback (tag 81, spec 17 §17.10) -----------------------------------------------------

pub(super) fn apply_lineage_rollback_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    payload: &PalwLineageRollbackV1,
    filer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let line_id = payload.line_id;
    let mut line = builder.improvement_line_row_v1(&line_id)?;
    let last = builder.state.improvement_last_head(&line_id).ok_or_else(|| refused("the line has no head history"))?;
    if last.cause != PalwHeadCauseV1::Promoted
        || last.epoch != payload.epoch
        || last.previous != Some(payload.to_class)
        || last.class_id != line.head
    {
        return Err(refused("only the latest promotion, restoring its parent, can be rolled back"));
    }
    // [E22] The window and the bar are the ones pinned at the promotion.
    let terms =
        line.last_promotion.filter(|p| p.epoch == payload.epoch).ok_or_else(|| refused("the promotion is not the line's latest"))?;
    let cause = match payload.cause {
        PalwRollbackCauseV1::Owner => {
            let owner = builder.state.model_line_or_founding(&line_id).and_then(|l| l.owner);
            if owner != Some(*filer) {
                return Err(refused("an owner's rollback filed by another bond"));
            }
            if daa > terms.owner_until_daa {
                return Err(refused("the owner's rollback window has passed"));
            }
            PalwHeadCauseV1::RolledBackByOwner
        }
        PalwRollbackCauseV1::LaterRegression { epoch } => {
            let check = builder.state.improvement_epoch(&line_id, epoch).ok_or_else(|| refused("no such regression-check epoch"))?;
            let shown = epoch > payload.epoch
                && check.is_decided()
                && check.previous == Some(payload.to_class)
                && check.previous_counts.is_some_and(|c| c.eligible);
            if !shown {
                return Err(refused("the named epoch shows no regression of the promoted class"));
            }
            PalwHeadCauseV1::RolledBackByProof
        }
        PalwRollbackCauseV1::CanaryFailed { .. } => {
            return Err(refused("a canary rollback needs the evaluation lane's canary jobs (not in v1)"));
        }
        PalwRollbackCauseV1::LicenceViolation { .. } => {
            return Err(refused("a licence rollback needs a challenge procedure (not in v1)"));
        }
    };
    // [D12] Vesting is final: every grant first vests to this DAA, and only the remainder is forfeited.
    vest_improvement_grants_v1(builder, &line_id, daa)?;
    // E11: an open epoch is aborted first, against the head it opened on.
    if let Some(open) = line.open_epoch {
        decide_epoch_v1(builder, ctx, &mut line, open, PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::Aborted })?;
    }
    // [D7] Every grant of the promoted epoch — the trainer's and the datasets' S2 and the winner's bond —
    // forfeits its unvested remainder (PALW-MIP-16); the submitter is barred.
    let promoted_epoch = payload.epoch;
    for (index, grant) in
        builder.state.improvement_grants(&line_id, promoted_epoch).into_iter().map(|(i, g)| (i, *g)).collect::<Vec<_>>()
    {
        if grant.forfeited || grant.vested >= grant.amount {
            continue;
        }
        let left = grant.amount - grant.vested;
        let mut pool = builder.improvement_pool_of_v1(&line_id);
        pool.unvested = pool.unvested.saturating_sub(left);
        pool.balance = pool.balance.saturating_add(left);
        pool.forfeited_in += left as u128;
        builder.write_improvement_pool(line_id, Some(pool));
        builder.write_improvement_grant((line_id, promoted_epoch, index), Some(PalwRewardGrantV1 { forfeited: true, ..grant }));
    }
    if let Some((_, winner)) = builder.state.improvement_candidate(&line_id, promoted_epoch, &last.class_id) {
        let until = daa.saturating_add(terms.ban_daa);
        let submitter = winner.submitter;
        line.barred.retain(|(bond, _)| *bond != submitter);
        line.barred.push((submitter, until));
        if line.barred.len() > PALW_IMPROVE_BARRED_MAX_V1 {
            line.barred.sort_by_key(|(_, until)| std::cmp::Reverse(*until));
            line.barred.truncate(PALW_IMPROVE_BARRED_MAX_V1);
        }
    }
    move_head_v1(builder, &mut line, promoted_epoch, payload.to_class, daa, cause);
    // The promotion can no longer be named: its rows retire once their grants settle.
    line.regression_check = None;
    line.last_promotion = None;
    line.regression_epoch = None;
    // [D12] The next check, vesting step or opt-out, whichever is first.
    line.next_due_daa = next_due_v1(builder, &line, daa)?;
    builder.write_improvement_line(line_id, Some(line));
    Ok(())
}

// ---- the sweep (spec 17 §17.5.2) ---------------------------------------------------------------

/// **Step 2's improvement sweep**: the bounded retirement, every due line's transitions and vesting,
/// then the bounded flush of the earnings ledger. Nothing below the fence (the tables are empty there).
pub(super) fn advance_improvement_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !builder.params.improve_active_at(ctx.daa_score) {
        return Ok(());
    }
    retire_improvement_rows_v1(builder);
    let due: Vec<Hash64> =
        builder.state.improvement_due.iter().take_while(|(at, _)| *at <= ctx.daa_score).map(|(_, line_id)| *line_id).collect();
    for line_id in due {
        advance_improvement_line_v1(builder, ctx, line_id)?;
    }
    flush_improvement_earnings_v1(builder, ctx);
    Ok(())
}

/// The bounded retirement of decided epochs' detail rows (spec 17 §17.5.4): pool entries, then items,
/// then results, in key order, oldest decided epoch first.
fn retire_improvement_rows_v1(builder: &mut TransitionBuilder<'_>) {
    let mut budget = PALW_IMPROVE_RETIRE_ROWS_PER_BLOCK_V1;
    while budget > 0 {
        let Some(&(_, line_id, epoch)) = builder.state.improvement_retiring.iter().next() else { break };
        let pool: Vec<_> =
            builder.state.improvement_pool_entries.range(pool_range(&line_id, epoch)).map(|(k, _)| *k).take(budget).collect();
        budget -= pool.len();
        for key in pool {
            builder.write_improvement_pool_entry(key, None);
        }
        let items: Vec<_> = builder
            .state
            .improvement_items
            .range((line_id, epoch, 0)..=(line_id, epoch, u32::MAX))
            .map(|(k, _)| *k)
            .take(budget)
            .collect();
        budget -= items.len();
        for key in items {
            builder.write_improvement_item(key, None);
        }
        let lo = (line_id, epoch, 0, PalwEvalSubjectV1::Parent);
        let hi = (line_id, epoch, u32::MAX, PalwEvalSubjectV1::Previous(Hash64::from_bytes([0xFF; 64])));
        let results: Vec<_> = builder.state.improvement_results.range(lo..=hi).map(|(k, _)| *k).take(budget).collect();
        budget -= results.len();
        for key in results {
            builder.write_improvement_result(key, None);
        }
        // The evaluation lane's job rows (A6), in the same budget.
        let (jobs, jobs_left) = builder.retire_improvement_eval_jobs_v1(&line_id, epoch, budget);
        budget -= jobs;
        let remaining = builder.state.improvement_pool_entries.range(pool_range(&line_id, epoch)).next().is_some()
                || builder.state.improvement_items.range((line_id, epoch, 0)..=(line_id, epoch, u32::MAX)).next().is_some()
                || builder.state.improvement_results.range(lo..=hi).next().is_some()
                || jobs_left;
        if remaining {
            break;
        }
        if let Some(mut header) = builder.state.improvement_epochs.get(&(line_id, epoch)).cloned() {
            header.retire = PalwEpochRetireV1::Done;
            builder.write_improvement_epoch((line_id, epoch), Some(header));
        } else {
            break;
        }
    }
}

/// The bounded flush of the earnings ledger into payout rows (spec 17 §17.11.5): at most two per
/// block, and never into the last `PALW_IMPROVE_PAYOUT_QUEUE_RESERVE_V1` free rows of the queue. A bond
/// that no longer exists has nobody to pay: its earnings are burned.
fn flush_improvement_earnings_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    for _ in 0..PALW_IMPROVE_PAYOUTS_PER_BLOCK_V1 {
        if builder.state.pending_payouts.len() + PALW_IMPROVE_PAYOUT_QUEUE_RESERVE_V1 >= PALW_V2_MAX_PENDING_PAYOUTS {
            return;
        }
        let Some((&bond, &amount)) = builder.state.improvement_earnings.iter().next() else { return };
        builder.write_improvement_earning(bond, None);
        let Some(payload) = builder.state.bonds.get(&bond).map(|record| record.payout_payload) else { continue };
        let mut h =
            blake2b_simd::Params::new().hash_length(64).key(crate::palw_improve_policy_v1::PALW_IMPROVE_PAYOUT_DOMAIN_V1).to_state();
        h.update(&borsh::to_vec(&bond).expect("a bond key is borsh-serializable"));
        h.update(&ctx.daa_score.to_le_bytes());
        h.update(ctx.block.as_byte_slice());
        let mut key = [0u8; 64];
        key.copy_from_slice(h.finalize().as_bytes());
        key[0] = PALW_STATE_V2_IMPROVE_PAYOUT_KEY_PREFIX;
        builder.write_payout(Hash64::from_bytes(key), Some(PalwPayoutV2 { payload, amount }));
    }
}

/// Advance one line (spec 17 §17.5.2).
fn advance_improvement_line_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    line_id: Hash64,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let mut line = builder.improvement_line_row_v1(&line_id)?;
    if line.status == PalwImprovementLineStatusV1::Dissolved {
        return Ok(());
    }
    line.barred.retain(|(_, until)| *until > daa);
    // [D12] Vesting is final and runs first: every grant pays up to this DAA before any transition.
    vest_improvement_grants_v1(builder, &line_id, daa)?;
    for _ in 0..PALW_IMPROVE_STEPS_PER_ADVANCE_V1 {
        let progressed = match line.open_epoch {
            Some(epoch) => step_epoch_v1(builder, ctx, &mut line, epoch)?,
            None => step_idle_v1(builder, ctx, &mut line)?,
        };
        if !progressed {
            break;
        }
    }
    vest_improvement_grants_v1(builder, &line_id, daa)?;
    retire_settled_epochs_v1(builder, &line, daa);
    if dissolve_if_done_v1(builder, &mut line, daa)? {
        builder.write_improvement_line(line_id, Some(line));
        return Ok(());
    }
    line.next_due_daa = next_due_v1(builder, &line, daa)?;
    builder.write_improvement_line(line_id, Some(line));
    Ok(())
}

/// When the line is next due (spec 17 §17.5.2).
fn next_due_v1(builder: &TransitionBuilder<'_>, line: &PalwImprovementLineV1, daa: u64) -> Result<u64, PalwStateV2Error> {
    let policy = builder.improvement_policy_of_v1(&line.line_id)?;
    let boundary = palw_improve_next_boundary_v1(daa, policy.windows.grid);
    let mut due = match line.open_epoch {
        None => boundary,
        Some(epoch) => {
            let header = builder.improvement_epoch_of_v1(&line.line_id, epoch)?;
            let t = header.times;
            match header.state {
                PalwEpochStateV1::Open => t.t_fix,
                PalwEpochStateV1::Submission => t.t_close,
                PalwEpochStateV1::HoldOut => t.t_draw,
                PalwEpochStateV1::Drawing => t.t_draw.saturating_add(policy.windows.beacon_delay),
                PalwEpochStateV1::Evaluating => t.t_eval,
                // Every block while closing: the evaluation lane may report completion before t_score.
                PalwEpochStateV1::Closing => (daa + 1).min(t.t_score),
                PalwEpochStateV1::Decided => boundary,
            }
        }
    };
    if let PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(at) } = line.status {
        due = due.min(at.max(daa + 1));
    }
    // [D12] The next vesting step of any grant still vesting (§17.5.2).
    if let Some(step) = palw_improve_next_vesting_step_v1(&builder.state, &line.line_id, daa) {
        due = due.min(step);
    }
    Ok(due.max(daa + 1))
}

/// An idle line at a grid boundary: open an epoch when the trigger fires (spec 17 §17.5.3 step 1).
fn step_idle_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    line: &mut PalwImprovementLineV1,
) -> Result<bool, PalwStateV2Error> {
    let daa = ctx.daa_score;
    if line.status != PalwImprovementLineStatusV1::Governed || daa < line.next_due_daa {
        return Ok(false);
    }
    let policy = builder.improvement_policy_of_v1(&line.line_id)?;
    let g = palw_improve_grid_floor_v1(daa, policy.windows.grid);
    let usage = builder.state.improvement_usage.get(&line.line_id).copied().unwrap_or_default();
    let ceilings = builder.improvement_ceilings_v1()?;
    let results_bound = palw_improvement_results_bound_v1(&policy);
    if usage.usage < policy.usage.value
        || builder.state.improvement_open_epochs >= ceilings.max_open_epochs
        || builder.state.improvement_live_results.saturating_add(results_bound) > ceilings.max_live_results as u64
    {
        return Ok(false);
    }
    let epoch = line.next_epoch;
    // [E20] The regression check is taken at the draw, not here: an epoch that ends before its draw
    // leaves it for the next.
    let previous = None;
    let header = PalwImprovementEpochV1 {
        line_id: line.line_id,
        epoch,
        state: PalwEpochStateV1::Open,
        times: palw_improve_epoch_times_v1(g, &policy.windows),
        parent: line.head,
        previous,
        policy_digest: line.policy_digest,
        dataset_root: None,
        candidates: 0,
        pool_entries: 0,
        holdout_cases: 0,
        setter_sets: 0,
        seed: None,
        items: 0,
        results_bound: u32::try_from(results_bound).unwrap_or(u32::MAX),
        previous_counts: None,
        outcome: None,
        escrow: PalwEpochEscrowV1::default(),
        grants: 0,
        decided_daa: None,
        retire: PalwEpochRetireV1::Pending,
    };
    builder.write_improvement_epoch((line.line_id, epoch), Some(header));
    builder.write_improvement_usage(line.line_id, Some(PalwImprovementUsageV1 { usage: 0, since_daa: daa }));
    let mut pool = builder.improvement_pool_of_v1(&line.line_id);
    pool.s1_budget = ((pool.balance as u128 * policy.bounty_share_permille as u128) / 1000) as u64;
    builder.write_improvement_pool(line.line_id, Some(pool));
    line.open_epoch = Some(epoch);
    line.next_epoch = epoch + 1;
    Ok(true)
}

/// One transition of an open epoch, if one is due (spec 17 §17.5.3 steps 2–7).
fn step_epoch_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    line: &mut PalwImprovementLineV1,
    epoch: u64,
) -> Result<bool, PalwStateV2Error> {
    let daa = ctx.daa_score;
    let line_id = line.line_id;
    let mut header = builder.improvement_epoch_of_v1(&line_id, epoch)?;
    let policy = builder.improvement_policy_of_v1(&line_id)?;
    let t = header.times;
    match header.state {
        PalwEpochStateV1::Open if daa >= t.t_fix => {
            let material = builder.state.improvement_material.get(&(line_id, epoch)).cloned().unwrap_or_default();
            header.dataset_root = Some(palw_improve_dataset_root_v1(&material));
            header.state = PalwEpochStateV1::Submission;
        }
        PalwEpochStateV1::Submission if daa >= t.t_close => {
            if header.candidates == 0 {
                decide_epoch_v1(
                    builder,
                    ctx,
                    line,
                    epoch,
                    PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::NoCandidate },
                )?;
                return Ok(true);
            }
            header.state = PalwEpochStateV1::HoldOut;
        }
        PalwEpochStateV1::HoldOut if daa >= t.t_draw => header.state = PalwEpochStateV1::Drawing,
        PalwEpochStateV1::Drawing if daa >= t.t_draw.saturating_add(policy.windows.beacon_delay) => {
            return draw_epoch_v1(builder, ctx, line, header, &policy);
        }
        PalwEpochStateV1::Evaluating if daa >= t.t_eval => header.state = PalwEpochStateV1::Closing,
        PalwEpochStateV1::Closing
            if daa >= t.t_score
                || !(palw_improve_eval_pending_hook_v1(&builder.state, &line_id, epoch)
                    || palw_improve_material_pending_hook_v1(&builder.state, &line_id, epoch)) =>
        {
            palw_improve_material_before_scoring_hook_v1(builder, &line_id, epoch)?;
            let outcome = score_epoch_v1(builder, &line_id, &header, &policy)?;
            decide_epoch_v1(builder, ctx, line, epoch, outcome)?;
            return Ok(true);
        }
        _ => return Ok(false),
    }
    builder.write_improvement_epoch((line_id, epoch), Some(header));
    Ok(true)
}

/// The draw (spec 17 §17.8.1) and the parent's escrow (§17.11.2).
fn draw_epoch_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    line: &mut PalwImprovementLineV1,
    mut header: PalwImprovementEpochV1,
    policy: &PalwImprovementPolicyV1,
) -> Result<bool, PalwStateV2Error> {
    let (line_id, epoch) = (line.line_id, header.epoch);
    let seed = palw_improve_epoch_seed_v1(&ctx.block, &line_id, epoch);
    let mut entries = Vec::new();
    let mut sources = Vec::new();
    for entry in builder.state.improvement_pool_entries(&line_id, epoch) {
        match *entry {
            PalwPoolEntryV2::HoldOut { id, supplier } => {
                entries.push(PalwDrawEntryV1 { id, supplier });
                sources.push(PalwItemSourceV1::HoldOut);
            }
            PalwPoolEntryV2::SetterSet { set_id, setter, items } => {
                for index in 0..items {
                    entries.push(PalwDrawEntryV1 { id: palw_improve_setter_item_id_v1(&set_id, index), supplier: setter });
                    sources.push(PalwItemSourceV1::Setter { set_id, index });
                }
            }
        }
    }
    let drawn = palw_improve_draw_v1(&seed, &entries, policy.eval.n, policy.eval.setter_cap_permille);
    if (drawn.len() as u32) < policy.eval.n_min {
        header.seed = Some(seed);
        builder.write_improvement_epoch((line_id, epoch), Some(header));
        decide_epoch_v1(builder, ctx, line, epoch, PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::TooFewItems })?;
        return Ok(true);
    }
    // [E24] `TooFewItems` is decided before `PoolInsufficient`.
    // [E20] The regression check, if one is owed and its class is still admitted, joins here.
    let previous = line.regression_check.filter(|class| builder.state.tir_classes.contains_key(class));
    let fee = policy.fees.eval_fee_per_job;
    let parent_escrow = palw_improvement_jobs_per_subject_v1(&policy.eval, false).saturating_mul(fee);
    let previous_escrow =
        if previous.is_some() { palw_improvement_jobs_per_subject_v1(&policy.eval, true).saturating_mul(fee) } else { 0 };
    let need = parent_escrow.saturating_add(previous_escrow);
    let mut pool = builder.improvement_pool_of_v1(&line_id);
    if pool.balance < need {
        header.seed = Some(seed);
        builder.write_improvement_epoch((line_id, epoch), Some(header));
        decide_epoch_v1(
            builder,
            ctx,
            line,
            epoch,
            PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::PoolInsufficient },
        )?;
        return Ok(true);
    }
    pool.balance -= need;
    pool.held += need;
    builder.write_improvement_pool(line_id, Some(pool));
    header.escrow = PalwEpochEscrowV1 { parent: parent_escrow, parent_spent: 0, previous: previous_escrow, previous_spent: 0 };
    if previous.is_some() {
        header.previous = previous;
        line.regression_check = None;
        line.regression_epoch = Some(epoch);
    }
    let judged = palw_improvement_has_stage_v1(&policy.eval, PalwScoringKindV1::Judge)
        || palw_improvement_has_stage_v1(&policy.eval, PalwScoringKindV1::Pairwise);
    let mut item = 0u32;
    for index in drawn {
        let judge = if judged {
            palw_improve_judge_index_v1(&seed, item, policy.eval.judge_set.len()).map(|j| policy.eval.judge_set[j])
        } else {
            None
        };
        builder.write_improvement_item(
            (line_id, epoch, item),
            Some(PalwEvalItemV1 {
                item,
                case_id: entries[index].id,
                source: sources[index],
                supplier: Some(entries[index].supplier),
                seed: palw_improve_eval_seed_v1(&seed, item),
                judge,
                dropped: false,
            }),
        );
        item += 1;
    }
    for (root, count, regression) in [
        (policy.eval.regression_suite_root, policy.eval.regression_items, true),
        (policy.eval.safety_suite_root, policy.eval.safety_items, false),
    ] {
        for index in 0..count {
            builder.write_improvement_item(
                (line_id, epoch, item),
                Some(PalwEvalItemV1 {
                    item,
                    case_id: palw_improve_suite_item_id_v1(&root, index),
                    source: if regression { PalwItemSourceV1::Regression { index } } else { PalwItemSourceV1::Safety { index } },
                    supplier: None,
                    seed: palw_improve_eval_seed_v1(&seed, item),
                    judge: None,
                    dropped: false,
                }),
            );
            item += 1;
        }
    }
    header.seed = Some(seed);
    header.items = item;
    header.state = PalwEpochStateV1::Evaluating;
    builder.write_improvement_epoch((line_id, epoch), Some(header));
    Ok(true)
}

/// **Scoring** (spec 17 §17.9): every candidate's counts and eligibility (written on its row), the
/// regression check's counts (on the header), and the decision.
fn score_epoch_v1(
    builder: &mut TransitionBuilder<'_>,
    line_id: &Hash64,
    header: &PalwImprovementEpochV1,
    policy: &PalwImprovementPolicyV1,
) -> Result<PalwPromotionOutcomeV1, PalwStateV2Error> {
    let epoch = header.epoch;
    let items: Vec<PalwEvalItemV1> = builder.state.improvement_items(line_id, epoch).into_iter().copied().collect();
    let subjects = builder.state.improvement_subjects(line_id, epoch);
    let rule = PalwImproveRuleV1 {
        n_min: policy.eval.n_min,
        delta_permille: policy.eval.delta_permille,
        epsilon_permille: policy.eval.epsilon_permille,
        epsilon_safety_permille: policy.eval.epsilon_safety_permille,
        alpha_permille: policy.eval.alpha_permille,
        has_judge: palw_improvement_has_stage_v1(&policy.eval, PalwScoringKindV1::Judge),
        has_pairwise: palw_improvement_has_stage_v1(&policy.eval, PalwScoringKindV1::Pairwise),
    };
    let candidates: Vec<(u32, PalwEpochCandidateV1)> =
        builder.state.improvement_candidates(line_id, epoch).into_iter().map(|(i, row)| (i, row.clone())).collect();
    let k_count = candidates.len() as u32;
    let (counted, previous_counts) = {
        let scores = EpochScores { state: &builder.state, line_id: *line_id, epoch };
        let excluded_judges: Vec<Hash64> = if rule.has_judge {
            policy
                .eval
                .judge_set
                .iter()
                .filter(|judge| palw_improve_judge_excluded_v1(&scores, &items, &subjects, judge, policy.eval.anchor_floor_permille))
                .copied()
                .collect()
        } else {
            Vec::new()
        };
        let excluded = |judge: &Hash64| excluded_judges.contains(judge);
        let counted: Vec<(u32, PalwEpochCandidateV1, PalwPromotionCountsV1)> = candidates
            .into_iter()
            .map(|(index, row)| {
                let mut counts =
                    palw_improve_counts_v1(&scores, &items, &PalwEvalSubjectV1::Candidate(row.class_id), &rule, &excluded);
                counts.eligible = palw_improve_eligible_v1(&counts, &rule, k_count);
                (index, row, counts)
            })
            .collect();
        let previous_counts = header.previous.map(|class| {
            let mut counts = palw_improve_counts_v1(&scores, &items, &PalwEvalSubjectV1::Previous(class), &rule, &excluded);
            counts.eligible = palw_improve_eligible_v1(&counts, &rule, 1);
            counts
        });
        (counted, previous_counts)
    };
    for (index, row, counts) in &counted {
        builder.write_improvement_candidate(
            (*line_id, epoch, *index),
            Some(PalwEpochCandidateV1 { counts: Some(*counts), ..row.clone() }),
        );
    }
    if let Some(counts) = previous_counts {
        let mut header = builder.improvement_epoch_of_v1(line_id, epoch)?;
        header.previous_counts = Some(counts);
        builder.write_improvement_epoch((*line_id, epoch), Some(header));
    }
    let list: Vec<(Hash64, PalwPromotionCountsV1)> = counted.iter().map(|(_, row, counts)| (row.class_id, *counts)).collect();
    Ok(palw_improve_decide_v1(&list, &rule))
}

/// **The epoch's end** (spec 17 §17.5.3 step 8, §17.9.5, §17.10.3, §17.11): the decision, the money,
/// the head, the line back to idle.
fn decide_epoch_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    line: &mut PalwImprovementLineV1,
    epoch: u64,
    outcome: PalwPromotionOutcomeV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let line_id = line.line_id;
    let policy = builder.improvement_policy_of_v1(&line_id)?;
    let mut header = builder.improvement_epoch_of_v1(&line_id, epoch)?;
    let aborted = matches!(outcome, PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::Aborted });
    let winner = match outcome {
        PalwPromotionOutcomeV1::Promoted { class_id, .. } => Some(class_id),
        PalwPromotionOutcomeV1::NoChange { .. } => None,
    };
    let unit = palw_improvement_epoch_length_v1(&policy.windows);
    let mut grants = Vec::new();
    // [D3] The parent's and the regression check's unspent escrow return to the balance first (the
    // spent part is sunk); S2's `R` below is taken from the balance they leave.
    let spent = header.escrow.parent_spent + header.escrow.previous_spent;
    let unspent = (header.escrow.parent - header.escrow.parent_spent) + (header.escrow.previous - header.escrow.previous_spent);
    if unspent > 0 {
        let mut pool = builder.improvement_pool_of_v1(&line_id);
        pool.held -= unspent;
        pool.balance += unspent;
        builder.write_improvement_pool(line_id, Some(pool));
    }
    let candidates: Vec<PalwEpochCandidateV1> =
        builder.state.improvement_candidates(&line_id, epoch).into_iter().map(|(_, row)| row.clone()).collect();
    // [D2/E21] The fees funded the draw's escrow with the rest of the balance: on an abort each fee is
    // refunded less its share of what the parent's and `Previous`'s escrow spent,
    // `fee_i − ⌈fee_i · min(F, S) / F⌉`, the rounding staying in the balance — which never goes below
    // zero.
    let fees: u128 = candidates.iter().map(|row| row.fee_paid as u128).sum();
    let consumed = fees.min(spent as u128);
    for row in &candidates {
        let unspent = row.escrow - row.escrow_spent;
        let refund = if aborted {
            // E11: the bond and what is left of the escrow in full, and the fee's unspent part.
            let fee_share = if fees == 0 { 0 } else { (row.fee_paid as u128 * consumed).div_ceil(fees) as u64 };
            let mut pool = builder.improvement_pool_of_v1(&line_id);
            // [D2/E21] And never more than the balance holds: an S1 payout may have spent part of what the
            // fees funded, and the abort — the owner's safety valve — is not refused for want of balance;
            // a candidate later in acceptance order then gets back what is left.
            let fee_refund = (row.fee_paid - fee_share.min(row.fee_paid)).min(pool.balance);
            pool.balance -= fee_refund;
            pool.held = pool.held.checked_add(fee_refund).ok_or(PalwStateV2Error::Overflow("improvement held"))?;
            builder.write_improvement_pool(line_id, Some(pool));
            row.bond + unspent + fee_refund
        } else if Some(row.class_id) == winner {
            // The winner's bond vests (below); its unspent escrow comes back; its fee stays.
            let mut pool = builder.improvement_pool_of_v1(&line_id);
            pool.held -= row.bond;
            pool.unvested += row.bond;
            builder.write_improvement_pool(line_id, Some(pool));
            grants.push(PalwRewardGrantV1 {
                recipient: row.submitter,
                stage: PalwRewardStageV1::WinnerBond,
                amount: row.bond,
                label: PalwTrustLabelV1::Bonded,
                vest_from_daa: daa,
                vest_unit_daa: unit,
                vest_epochs: policy.vest_epochs,
                vested: 0,
                forfeited: false,
            });
            unspent
        } else {
            // A loser's bond and unspent escrow come back; its fee stays.
            row.bond + unspent
        };
        builder.release_improvement_hold_v1(&line_id, &row.submitter, refund)?;
    }
    if let Some(class_id) = winner {
        // S2 (§17.11.3).
        let mut pool = builder.improvement_pool_of_v1(&line_id);
        let reward = ((pool.balance as u128 * policy.promotion_share_permille as u128) / 1000) as u64;
        let trainer = ((reward as u128 * policy.s2_trainer_permille as u128) / 1000) as u64;
        let data = reward - trainer;
        let (_, winner_row) =
            builder.state.improvement_candidate(&line_id, epoch, &class_id).ok_or_else(|| refused("the winner is not a candidate"))?;
        let winner_row = winner_row.clone();
        let mut granted = 0u64;
        let vesting = |recipient: PalwBondKeyV2, stage: PalwRewardStageV1, amount: u64, grants: &mut Vec<PalwRewardGrantV1>| {
            if amount > 0 {
                grants.push(PalwRewardGrantV1 {
                    recipient,
                    stage,
                    amount,
                    label: PalwTrustLabelV1::Trusted,
                    vest_from_daa: daa,
                    vest_unit_daa: unit,
                    vest_epochs: policy.vest_epochs,
                    vested: 0,
                    forfeited: false,
                });
            }
            amount
        };
        granted += vesting(winner_row.submitter, PalwRewardStageV1::S2Trainer, trainer, &mut grants);
        let dataset_cap = ((reward as u128 * policy.s2_dataset_cap_permille as u128) / 1000) as u64;
        let contributor_cap = ((reward as u128 * policy.s2_contributor_cap_permille as u128) / 1000) as u64;
        let mut per_contributor: Vec<(PalwBondKeyV2, u64)> = Vec::new();
        for (dataset_id, weight) in &winner_row.datasets {
            let Some(contributor) = palw_improve_dataset_registrant_hook_v1(&builder.state, &line_id, dataset_id) else { continue };
            let share = (((data as u128) * (*weight as u128)) / 1000) as u64;
            let taken = per_contributor.iter().find(|(b, _)| *b == contributor).map_or(0, |(_, t)| *t);
            let amount = share.min(dataset_cap).min(contributor_cap.saturating_sub(taken));
            match per_contributor.iter_mut().find(|(b, _)| *b == contributor) {
                Some((_, t)) => *t += amount,
                None => per_contributor.push((contributor, amount)),
            }
            granted += vesting(contributor, PalwRewardStageV1::S2Dataset, amount, &mut grants);
        }
        pool.balance -= granted;
        pool.unvested += granted;
        builder.write_improvement_pool(line_id, Some(pool));
        move_head_v1(builder, line, epoch, class_id, daa, PalwHeadCauseV1::Promoted);
        // [E22] The rollback terms are the promoted epoch's policy's, pinned now.
        line.last_promotion = Some(PalwLastPromotionV1 {
            epoch,
            owner_until_daa: daa.saturating_add(unit.saturating_mul(policy.rollback_epochs as u64)),
            ban_daa: unit.saturating_mul(policy.ban_epochs as u64),
        });
        line.regression_check = Some(header.parent);
        line.regression_epoch = None;
    }
    for grant in grants {
        builder.write_improvement_grant((line_id, epoch, header.grants), Some(grant));
        header.grants += 1;
    }
    // Re-read: scoring may have written the header (the regression check's counts).
    let written = builder.improvement_epoch_of_v1(&line_id, epoch)?;
    header.previous_counts = written.previous_counts;
    header.state = PalwEpochStateV1::Decided;
    header.outcome = Some(outcome);
    header.decided_daa = Some(daa);
    header.retire = PalwEpochRetireV1::Pending;
    builder.write_improvement_epoch((line_id, epoch), Some(header));
    // The line is idle again: a pending policy comes into force, a pending opt-out starts its delay.
    line.open_epoch = None;
    if let Some(mut record) = builder.state.improvement_policies.get(&line_id).cloned()
        && let Some(pending) = record.pending.take()
    {
        record.policy = *pending;
        line.policy_digest = palw_improvement_policy_digest_v1(&record.policy);
        builder.write_improvement_policy(line_id, Some(record));
    }
    if let PalwImprovementLineStatusV1::OptingOut { effective_daa: None } = line.status {
        line.status = PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(daa.saturating_add(policy.windows.grid)) };
    }
    line.next_due_daa = palw_improve_next_boundary_v1(daa, builder.improvement_policy_of_v1(&line_id)?.windows.grid);
    Ok(())
}

/// **The next DAA at which one of the line's grants vests a step** (`from + k·L_e`, strictly after
/// `daa`), if any grant is still vesting.
fn palw_improve_next_vesting_step_v1(state: &PalwChainStateV2, line_id: &Hash64, daa: u64) -> Option<u64> {
    state
        .improvement_grants
        .range((*line_id, 0, 0)..=(*line_id, u64::MAX, u32::MAX))
        .filter(|(_, g)| !g.forfeited && g.vested < g.amount && g.vest_unit_daa > 0)
        .map(|(_, g)| {
            let done = daa.saturating_sub(g.vest_from_daa) / g.vest_unit_daa;
            g.vest_from_daa.saturating_add((done + 1).saturating_mul(g.vest_unit_daa))
        })
        .min()
}

/// **Vesting** (spec 17 §17.11.4): every grant of the line's kept epochs is paid up to its target.
fn vest_improvement_grants_v1(builder: &mut TransitionBuilder<'_>, line_id: &Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
    let grants: Vec<((Hash64, u64, u32), PalwRewardGrantV1)> = builder
        .state
        .improvement_grants
        .range((*line_id, 0, 0)..=(*line_id, u64::MAX, u32::MAX))
        .filter(|(_, g)| !g.forfeited && g.vested < g.amount)
        .map(|(k, g)| (*k, *g))
        .collect();
    for (key, mut grant) in grants {
        let units = if grant.vest_unit_daa == 0 {
            grant.vest_epochs as u64
        } else {
            daa.saturating_sub(grant.vest_from_daa) / grant.vest_unit_daa
        };
        let target = ((grant.amount as u128 * units.min(grant.vest_epochs as u64) as u128) / grant.vest_epochs.max(1) as u128) as u64;
        if target <= grant.vested {
            continue;
        }
        let pay = target - grant.vested;
        grant.vested = target;
        let mut pool = builder.improvement_pool_of_v1(line_id);
        pool.unvested = pool.unvested.saturating_sub(pay);
        pool.paid += pay as u128;
        builder.write_improvement_pool(*line_id, Some(pool));
        builder.credit_improvement_earnings_v1(&grant.recipient, pay);
        builder.write_improvement_grant(key, Some(grant));
    }
    Ok(())
}

/// Delete decided epochs whose detail rows are gone, whose grants are settled, and that no rollback
/// can name (spec 17 §17.5.4): the header, its candidates and its grants.
fn retire_settled_epochs_v1(builder: &mut TransitionBuilder<'_>, line: &PalwImprovementLineV1, _daa: u64) {
    let line_id = line.line_id;
    let decided: Vec<u64> = builder
        .state
        .improvement_epochs
        .range((line_id, 0)..=(line_id, u64::MAX))
        .filter(|(_, h)| h.is_decided() && h.retire == PalwEpochRetireV1::Done)
        .map(|((_, e), _)| *e)
        .collect();
    for epoch in decided {
        if line.last_promotion.is_some_and(|p| p.epoch == epoch) || Some(epoch) == line.regression_epoch {
            continue;
        }
        let settled = builder.state.improvement_grants(&line_id, epoch).iter().all(|(_, g)| g.forfeited || g.vested >= g.amount);
        if !settled {
            continue;
        }
        for (index, _) in
            builder.state.improvement_candidates(&line_id, epoch).into_iter().map(|(i, r)| (i, r.clone())).collect::<Vec<_>>()
        {
            builder.write_improvement_candidate((line_id, epoch, index), None);
        }
        for (index, _) in builder.state.improvement_grants(&line_id, epoch).into_iter().map(|(i, g)| (i, *g)).collect::<Vec<_>>() {
            builder.write_improvement_grant((line_id, epoch, index), None);
        }
        builder.write_improvement_material((line_id, epoch), None);
        builder.write_improvement_epoch((line_id, epoch), None);
    }
}

/// **Dissolution** (spec 17 §17.4.4, §17.11.5): past an opt-out's effective DAA, with no epoch open and
/// every grant settled, the balance goes to the spec 15 owner and the line's rows leave; the header stays
/// as `Dissolved` so its policy sequence continues.
fn dissolve_if_done_v1(
    builder: &mut TransitionBuilder<'_>,
    line: &mut PalwImprovementLineV1,
    daa: u64,
) -> Result<bool, PalwStateV2Error> {
    let PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(at) } = line.status else { return Ok(false) };
    if daa < at || line.open_epoch.is_some() {
        return Ok(false);
    }
    let line_id = line.line_id;
    let unsettled = builder
        .state
        .improvement_grants
        .range((line_id, 0, 0)..=(line_id, u64::MAX, u32::MAX))
        .any(|(_, g)| !g.forfeited && g.vested < g.amount);
    if unsettled {
        return Ok(false);
    }
    let pool = builder.improvement_pool_of_v1(&line_id);
    if pool.held > 0 {
        return Ok(false);
    }
    // Every decided epoch's detail rows must be gone first (the bounded sweep), or dissolving would
    // leave items and results under an epoch that no longer exists.
    if builder.state.improvement_epochs.range((line_id, 0)..=(line_id, u64::MAX)).any(|(_, e)| e.retire != PalwEpochRetireV1::Done) {
        return Ok(false);
    }
    if let Some(owner) = builder.state.model_line_or_founding(&line_id).and_then(|l| l.owner) {
        builder.credit_improvement_earnings_v1(&owner, pool.balance);
    }
    let epochs: Vec<u64> = builder.state.improvement_epochs.range((line_id, 0)..=(line_id, u64::MAX)).map(|((_, e), _)| *e).collect();
    for epoch in epochs {
        for (index, _) in
            builder.state.improvement_candidates(&line_id, epoch).into_iter().map(|(i, r)| (i, r.clone())).collect::<Vec<_>>()
        {
            builder.write_improvement_candidate((line_id, epoch, index), None);
        }
        for (index, _) in builder.state.improvement_grants(&line_id, epoch).into_iter().map(|(i, g)| (i, *g)).collect::<Vec<_>>() {
            builder.write_improvement_grant((line_id, epoch, index), None);
        }
        builder.write_improvement_epoch((line_id, epoch), None);
    }
    let materials: Vec<u64> =
        builder.state.improvement_material.range((line_id, 0)..=(line_id, u64::MAX)).map(|((_, e), _)| *e).collect();
    for epoch in materials {
        builder.write_improvement_material((line_id, epoch), None);
    }
    let heads: Vec<u32> = builder.state.improvement_heads.range((line_id, 0)..=(line_id, u32::MAX)).map(|((_, s), _)| *s).collect();
    for seq in heads {
        builder.write_improvement_head((line_id, seq), None);
    }
    builder.write_improvement_policy(line_id, None);
    builder.write_improvement_usage(line_id, None);
    builder.write_improvement_pool(line_id, None);
    line.status = PalwImprovementLineStatusV1::Dissolved;
    line.open_epoch = None;
    line.next_due_daa = u64::MAX;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;
    use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
    use crate::tx::TransactionOutpoint;

    const ACTIVE: u64 = 100;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn bond(byte: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
    }

    const OWNER: u8 = 1;
    const ALICE: u8 = 2;
    const BOB: u8 = 3;
    const CAROL: u8 = 4;
    const LINE: u8 = 0x10;
    const CAND_A: u8 = 0x11;
    const CAND_B: u8 = 0x12;

    fn params() -> PalwStateParamsV2 {
        let p = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle.state.clone().with_improve_from_daa(Some(ACTIVE)).with_improve_ceilings(Some(PALW_DRILL_IMPROVE_CEILINGS_V1))
    }

    /// A small policy: 8 items, n_min 4, no suites, one ExactMatch stage, a 1,000-DAA grid.
    fn policy() -> PalwImprovementPolicyV1 {
        let mut p = palw_improvement_policy_example_v1();
        p.eval.n = 8;
        p.eval.n_min = 4;
        p.eval.regression_items = 0;
        p.eval.regression_suite_root = Hash64::default();
        p.eval.safety_items = 0;
        p.eval.safety_suite_root = Hash64::default();
        p.eval.stages.truncate(1);
        p.eval.setter_cap_permille = 1_000;
        p.usage.value = 2;
        p.k_max = 2;
        p
    }

    fn genesis() -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        for class in [LINE, CAND_A, CAND_B] {
            s.tir_classes.insert(h(class), PalwTirClassRecordV1::test_row_v1(h(class)));
        }
        s.model_lines.insert(h(LINE), crate::palw_model_lines_v1::founding_line_v1(h(LINE), Some(bond(OWNER)), b"line".to_vec(), 0));
        for who in [OWNER, ALICE, BOB, CAROL] {
            s.bonds.insert(
                bond(who),
                palw_bond_state_from_registration_v2(&[who], &[who], 1_000_000_000_000_000, h(0x80 + who), 0, Default::default()),
            );
        }
        s
    }

    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h((daa % 251) as u8), daa_score: daa, blue_score: daa, subsidy: 0 }
    }

    /// Run `f` on a builder over `state` at `daa`, after that block's sweep, and return the state.
    fn at(state: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64, f: impl FnOnce(&mut TransitionBuilder<'_>)) -> PalwChainStateV2 {
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(state, p, false, false, false, false, &extras);
        advance_improvement_v1(&mut builder, &ctx(daa)).expect("the sweep");
        f(&mut builder);
        builder.checkpoint().0
    }

    fn set(line: u8, sequence: u64, policy: Option<PalwImprovementPolicyV1>) -> PalwImprovementPolicySetV1 {
        PalwImprovementPolicySetV1 { line_id: h(line), sequence, policy }
    }

    /// The pool's conservation (spec 17 §17.11.5).
    fn conserved(s: &PalwChainStateV2, line: &Hash64) {
        let pool = s.improvement_pool(line).unwrap();
        let inflow = pool.deposited + pool.fees_in + pool.phi_in + pool.held_in;
        let stock = pool.balance as u128 + pool.held as u128 + pool.unvested as u128 + pool.paid + pool.refunded;
        assert_eq!(inflow, stock, "the pool conserves: {pool:?}");
    }

    fn opted_in(daa: u64) -> PalwChainStateV2 {
        let p = params();
        at(&genesis(), &p, daa, |b| apply_improvement_policy_set_v1(b, &ctx(daa), &set(LINE, 1, Some(policy()))).expect("opt in"))
    }

    #[test]
    fn a_policy_opts_in_changes_and_opts_out_and_a_replay_is_refused() {
        let p = params();
        let s = opted_in(500);
        let line = s.improvement_line(&h(LINE)).unwrap().clone();
        assert_eq!(
            (line.head, line.policy_sequence, line.next_due_daa),
            (h(LINE), 1, 1_000),
            "the head is the line's class; due at the next boundary"
        );
        assert!(s.improvement_governed_at(&h(LINE), 500));
        assert_eq!(s.improvement_head_history(&h(LINE)).len(), 1);
        // A replay of sequence 1 is refused.
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(matches!(
            apply_improvement_policy_set_v1(&mut b, &ctx(600), &set(LINE, 1, Some(policy()))),
            Err(PalwStateV2Error::ImprovementPolicyRefused(_))
        ));
        // A bad policy is refused by name.
        let mut bad = policy();
        bad.windows.w_eval = bad.windows.beacon_delay + 32;
        assert!(apply_improvement_policy_set_v1(&mut b, &ctx(600), &set(LINE, 2, Some(bad))).is_err(), "E12");
        // A change while idle is in force at once.
        let mut other = policy();
        other.k_max = 1;
        let s = at(&s, &p, 600, |b| apply_improvement_policy_set_v1(b, &ctx(600), &set(LINE, 2, Some(other.clone()))).unwrap());
        assert_eq!(s.improvement_policy(&h(LINE)).unwrap().k_max, 1);
        // Opt out: effective one grid later; then the line dissolves, its header staying.
        let s = at(&s, &p, 700, |b| apply_improvement_policy_set_v1(b, &ctx(700), &set(LINE, 3, None)).unwrap());
        assert_eq!(
            s.improvement_line(&h(LINE)).unwrap().status,
            PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(1_700) }
        );
        let s = at(&s, &p, 1_000, |_| {});
        assert!(s.improvement_line(&h(LINE)).unwrap().open_epoch.is_none(), "no epoch opens while opting out");
        let s = at(&s, &p, 1_700, |_| {});
        let line = s.improvement_line(&h(LINE)).unwrap();
        assert_eq!(line.status, PalwImprovementLineStatusV1::Dissolved);
        assert!(s.improvement_policy(&h(LINE)).is_none() && s.improvement_pool(&h(LINE)).is_none());
        assert!(!s.improvement_governed_at(&h(LINE), 1_700));
        // It opts in again with the sequence continuing.
        let again = at(&s, &p, 1_800, |b| apply_improvement_policy_set_v1(b, &ctx(1_800), &set(LINE, 4, Some(policy()))).unwrap());
        assert!(again.improvement_governed_at(&h(LINE), 1_800));
    }

    #[test]
    fn a_developer_cannot_promote_on_a_governed_line() {
        let p = params();
        let s = opted_in(500);
        let extras = PalwTransitionExtrasV1::default();
        let b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(matches!(b.refuse_developer_promotion_v1(&h(LINE), 600), Err(PalwStateV2Error::ImprovementLineGoverned(_))));
        assert!(b.refuse_developer_promotion_v1(&h(CAND_A), 600).is_ok(), "an ungoverned line");
        assert!(b.refuse_developer_promotion_v1(&h(LINE), ACTIVE - 1).is_ok(), "below the fence nothing is governed");
    }

    /// One epoch, end to end: usage opens it; material fixes its root; two candidates enter; eight
    /// hold-out cases are drawn; A passes every item the parent fails, B ties; A is promoted.
    #[test]
    fn an_epoch_runs_from_usage_to_a_promotion() {
        let p = params();
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        // The boundary opens epoch 1.
        let s = at(&s, &p, 1_000, |b| {
            b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::Dataset, &h(0x40), &bond(CAROL), 1_000).unwrap();
        });
        let e = s.improvement_epoch(&h(LINE), 1).unwrap().clone();
        assert_eq!(
            (e.state, e.times.t_fix, e.times.t_close, e.times.t_draw, e.times.t_eval, e.times.t_score),
            (PalwEpochStateV1::Open, 1_200, 1_400, 1_500, 1_800, 1_950)
        );
        assert_eq!(s.improvement_usage(&h(LINE)).unwrap().usage, 0, "usage restarts at the opening");
        assert_eq!(s.improvement_material(&h(LINE), 1).unwrap().count, 1);
        // t_fix: the root is fixed; the candidates enter.
        let s = at(&s, &p, 1_200, |b| {
            for (class, who) in [(CAND_A, ALICE), (CAND_B, BOB)] {
                b.admit_improvement_candidate_v1(
                    &h(LINE),
                    1,
                    &h(class),
                    &bond(who),
                    crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(class) },
                    h(0x50),
                    Vec::new(),
                    1_200,
                )
                .unwrap();
            }
            assert!(
                b.admit_improvement_candidate_v1(
                    &h(LINE),
                    1,
                    &h(LINE),
                    &bond(CAROL),
                    crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(1) },
                    h(0x50),
                    Vec::new(),
                    1_200
                )
                .is_err(),
                "the head is no candidate"
            );
        });
        let e = s.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.state, PalwEpochStateV1::Submission);
        assert!(e.dataset_root.is_some());
        assert_eq!(e.candidates, 2);
        conserved(&s, &h(LINE));
        // t_close → HoldOut; eight cases from two suppliers.
        let s = at(&s, &p, 1_400, |b| {
            for i in 0..8u8 {
                let supplier = if i % 2 == 0 { CAROL } else { OWNER };
                let placed = b
                    .note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::HardCase, &h(0x60 + i), &bond(supplier), 1_400)
                    .unwrap();
                assert_eq!(placed, PalwMaterialPlacementV1::HoldOut { epoch: 1 });
            }
        });
        assert_eq!(s.improvement_epoch(&h(LINE), 1).unwrap().holdout_cases, 8);
        // t_draw + d → Evaluating with eight items.
        let s = at(&s, &p, 1_510, |_| {});
        let e = s.improvement_epoch(&h(LINE), 1).unwrap().clone();
        assert_eq!((e.state, e.items), (PalwEpochStateV1::Evaluating, 8));
        assert!(e.escrow.parent > 0);
        conserved(&s, &h(LINE));
        // Scores: the parent fails every item, A passes every one, B fails every one.
        let s = at(&s, &p, 1_600, |b| {
            for item in 0..8u32 {
                for (subject, value) in [
                    (PalwEvalSubjectV1::Parent, 0),
                    (PalwEvalSubjectV1::Candidate(h(CAND_A)), 1),
                    (PalwEvalSubjectV1::Candidate(h(CAND_B)), 0),
                ] {
                    let score = PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value };
                    b.record_improvement_score_v1(&h(LINE), 1, item, subject, score).unwrap();
                    assert!(b.pay_improvement_eval_fee_v1(&h(LINE), 1, &subject, &bond(CAROL)).unwrap() > 0);
                }
            }
            let dup = PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value: 1 };
            assert!(b.record_improvement_score_v1(&h(LINE), 1, 0, PalwEvalSubjectV1::Parent, dup).is_err(), "a score recorded twice");
        });
        conserved(&s, &h(LINE));
        // t_eval → Closing, and — nothing pending: every score is in, no evaluation claim is live
        // (the evaluation lane's hook) — scored and decided in the same block, not at t_score.
        let s = at(&s, &p, 1_800, |_| {});
        let e = s.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.state, PalwEpochStateV1::Decided);
        assert_eq!(e.outcome, Some(PalwPromotionOutcomeV1::Promoted { class_id: h(CAND_A), wins: 8, losses: 0 }));
        let line = s.improvement_line(&h(LINE)).unwrap();
        assert_eq!((line.head, line.open_epoch, line.regression_check), (h(CAND_A), None, Some(h(LINE))));
        assert_eq!(
            line.last_promotion,
            Some(PalwLastPromotionV1 { epoch: 1, owner_until_daa: 1_950 + 2 * 950, ban_daa: 8 * 950 }),
            "[E22] the rollback terms are pinned at the promotion"
        );
        assert_eq!(s.improvement_last_head(&h(LINE)).unwrap().cause, PalwHeadCauseV1::Promoted);
        let (_, b_row) = s.improvement_candidate(&h(LINE), 1, &h(CAND_B)).unwrap();
        assert!(!b_row.counts.unwrap().eligible);
        assert!(s.improvement_earnings(&bond(BOB)) > 0 || s.pending_payouts.len() > 0, "B's bond and unspent escrow came back");
        let stages: Vec<PalwRewardStageV1> = s.improvement_grants(&h(LINE), 1).iter().map(|(_, g)| g.stage).collect();
        assert!(stages.contains(&PalwRewardStageV1::WinnerBond) && stages.contains(&PalwRewardStageV1::S2Trainer));
        conserved(&s, &h(LINE));
        // The next epoch runs the regression check on the old head.
        let mut s = s;
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 1_950 });
        let s = at(&s, &p, 2_000, |_| {});
        let e2 = s.improvement_epoch(&h(LINE), 2).unwrap();
        assert_eq!((e2.parent, e2.previous), (h(CAND_A), None), "[E20] the check joins at the draw");
        // The detail rows of epoch 1 retire over the following blocks.
        assert!(s.improvement_items(&h(LINE), 1).is_empty(), "eight items and 24 results fit one block's budget");
        assert_eq!(s.improvement_epoch(&h(LINE), 1).unwrap().retire, PalwEpochRetireV1::Done);
        // [E20] Epoch 2 has no candidate and ends before its draw: the check stays owed to the next.
        let s = at(&s, &p, 2_450, |_| {});
        assert_eq!(
            s.improvement_epoch(&h(LINE), 2).unwrap().outcome,
            Some(PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::NoCandidate })
        );
        let line = s.improvement_line(&h(LINE)).unwrap();
        assert_eq!((line.regression_check, line.regression_epoch), (Some(h(LINE)), None));
        // [D12] Idle, the line is next due at its grants' first vesting step (1,950 + 950), which comes
        // before the next boundary (3,000); the step vests a quarter of every grant.
        assert_eq!(line.next_due_daa, 2_900);
        let s = at(&s, &p, 2_900, |_| {});
        assert!(s.improvement_grants(&h(LINE), 1).iter().all(|(_, g)| g.vested == g.amount / 4));
        assert_eq!(s.improvement_line(&h(LINE)).unwrap().next_due_daa, 3_000);
        conserved(&s, &h(LINE));
    }

    #[test]
    fn an_epoch_with_no_candidate_or_too_few_items_changes_nothing() {
        let p = params();
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        // A DAA jump across the whole epoch applies every transition in one block (E15): no candidate.
        let s1 = at(&s, &p, 1_000, |_| {});
        let s1 = at(&s1, &p, 1_450, |_| {});
        let e = s1.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.outcome, Some(PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::NoCandidate }));
        assert_eq!(s1.improvement_line(&h(LINE)).unwrap().next_due_daa, 2_000, "the next boundary strictly after the decision");
        // Candidates, but only two cases: too few at the draw; every candidate is refunded but its fee.
        let s2 = at(&s, &p, 1_000, |_| {});
        let s2 = at(&s2, &p, 1_200, |b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(1) },
                h(0x50),
                Vec::new(),
                1_200,
            )
            .unwrap();
        });
        let s2 = at(&s2, &p, 1_400, |b| {
            for i in 0..2u8 {
                b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::HardCase, &h(0x60 + i), &bond(CAROL), 1_400).unwrap();
            }
        });
        let s2 = at(&s2, &p, 1_510, |_| {});
        let e = s2.improvement_epoch(&h(LINE), 1).unwrap();
        assert_eq!(e.outcome, Some(PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::TooFewItems }));
        let pool = s2.improvement_pool(&h(LINE)).unwrap();
        assert_eq!(pool.held, 0, "nothing held after the decision");
        assert_eq!(pool.balance, policy().fees.registration_fee + 2 * policy().fees.hard_case_fee, "the fee and the cases' fees stay");
        conserved(&s2, &h(LINE));
    }

    #[test]
    fn an_owners_rollback_aborts_the_open_epoch_and_forfeits_the_winner() {
        let p = params();
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        // A promotion, as above, compressed.
        let s = at(&s, &p, 1_000, |_| {});
        let s = at(&s, &p, 1_200, |b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(1) },
                h(0x50),
                Vec::new(),
                1_200,
            )
            .unwrap();
        });
        let s = at(&s, &p, 1_400, |b| {
            for i in 0..8u8 {
                b.note_improvement_material_v1(
                    &h(LINE),
                    PalwMaterialKindV1::HardCase,
                    &h(0x60 + i),
                    &bond(if i % 2 == 0 { CAROL } else { OWNER }),
                    1_400,
                )
                .unwrap();
            }
        });
        let s = at(&s, &p, 1_510, |b| {
            for item in 0..8u32 {
                for (subject, value) in [(PalwEvalSubjectV1::Parent, 0), (PalwEvalSubjectV1::Candidate(h(CAND_A)), 1)] {
                    let score = PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value };
                    b.record_improvement_score_v1(&h(LINE), 1, item, subject, score).unwrap();
                }
            }
        });
        let mut s = at(&s, &p, 1_950, |_| {});
        assert_eq!(s.improvement_line(&h(LINE)).unwrap().head, h(CAND_A));
        // Epoch 2 opens, a candidate enters, and the owner rolls back inside the window.
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 1_950 });
        let s = at(&s, &p, 2_000, |_| {});
        let s = at(&s, &p, 2_200, |b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                2,
                &h(CAND_B),
                &bond(BOB),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(2) },
                h(0x50),
                Vec::new(),
                2_200,
            )
            .unwrap();
        });
        let bob_before = s.improvement_earnings(&bond(BOB));
        let rollback = PalwLineageRollbackV1 { line_id: h(LINE), epoch: 1, to_class: h(LINE), cause: PalwRollbackCauseV1::Owner };
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(apply_lineage_rollback_v1(&mut b, &ctx(2_300), &rollback, &bond(ALICE)).is_err(), "not the owner");
        let wrong = PalwLineageRollbackV1 { to_class: h(CAND_B), ..rollback };
        assert!(apply_lineage_rollback_v1(&mut b, &ctx(2_300), &wrong, &bond(OWNER)).is_err(), "not the promotion's parent");
        apply_lineage_rollback_v1(&mut b, &ctx(2_300), &rollback, &bond(OWNER)).unwrap();
        let s = b.checkpoint().0;
        let line = s.improvement_line(&h(LINE)).unwrap();
        assert_eq!((line.head, line.open_epoch), (h(LINE), None), "the head is restored and the open epoch aborted (E11)");
        assert!(line.is_barred(&bond(ALICE), 2_300), "the winner's submitter is barred");
        assert_eq!(
            s.improvement_epoch(&h(LINE), 2).unwrap().outcome,
            Some(PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::Aborted })
        );
        let fees = policy().fees;
        assert_eq!(
            s.improvement_earnings(&bond(BOB)) - bob_before,
            fees.registration_fee
                + fees.candidate_bond
                + palw_improvement_jobs_per_subject_v1(&policy().eval, true) * fees.eval_fee_per_job,
            "B is refunded in full"
        );
        assert!(
            s.improvement_grants(&h(LINE), 1)
                .iter()
                .filter(|(_, g)| matches!(g.stage, PalwRewardStageV1::S2Trainer | PalwRewardStageV1::WinnerBond))
                .all(|(_, g)| g.forfeited)
        );
        conserved(&s, &h(LINE));
    }

    /// [D2/E21, D12, E22] A promotion, then the owner shortens `rollback_epochs` and `ban_epochs`.
    /// (a) Epoch 2 draws with B's fee in the balance and pays three of the parent's jobs; the owner's
    /// rollback aborts it: B gets its bond and escrow back in full and its fee less the spend.
    /// (b) With no epoch 2, the grants vest their first quarter on their own step (2,900), and the
    /// owner rolls back at 2,920 — past the new policy's window (2,900), inside the pinned one (3,850):
    /// the quarter stays vested and the bar is the pinned one. The pool conserves throughout.
    #[test]
    fn an_abort_refunds_each_fee_less_its_spend_and_a_rollback_keeps_what_vested() {
        fn cases(b: &mut TransitionBuilder<'_>, base: u8, daa: u64) {
            for i in 0..8u8 {
                b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::HardCase, &h(base + i), &bond(CAROL), daa).unwrap();
            }
        }
        fn enter(b: &mut TransitionBuilder<'_>, epoch: u64, class: u8, who: u8, daa: u64) {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                epoch,
                &h(class),
                &bond(who),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(class) },
                h(0x50),
                Vec::new(),
                daa,
            )
            .unwrap();
        }
        let p = params();
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let s = at(&s, &p, 1_000, |_| {});
        let s = at(&s, &p, 1_200, |b| enter(b, 1, CAND_A, ALICE, 1_200));
        let s = at(&s, &p, 1_400, |b| cases(b, 0x60, 1_400));
        let s = at(&s, &p, 1_510, |b| {
            for item in 0..8u32 {
                for (subject, value) in [(PalwEvalSubjectV1::Parent, 0), (PalwEvalSubjectV1::Candidate(h(CAND_A)), 1)] {
                    let score = PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value };
                    b.record_improvement_score_v1(&h(LINE), 1, item, subject, score).unwrap();
                }
            }
        });
        let s = at(&s, &p, 1_950, |_| {});
        assert_eq!(s.improvement_line(&h(LINE)).unwrap().head, h(CAND_A));
        // [E22] A later policy shortens the owner's window (to 1,950 + 950) and the bar (to 950).
        let mut shorter = policy();
        shorter.rollback_epochs = 1;
        shorter.ban_epochs = 1;
        let promoted = at(&s, &p, 1_960, |b| apply_improvement_policy_set_v1(b, &ctx(1_960), &set(LINE, 2, Some(shorter))).unwrap());
        let rollback = PalwLineageRollbackV1 { line_id: h(LINE), epoch: 1, to_class: h(LINE), cause: PalwRollbackCauseV1::Owner };
        let extras = PalwTransitionExtrasV1::default();
        let fees = policy().fees;
        let fee_per_job = fees.eval_fee_per_job;

        // (a) [D2/E21] Epoch 2 draws; three of the parent's jobs are paid; the owner aborts it.
        let mut s = promoted.clone();
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 1_960 });
        let s = at(&s, &p, 2_000, |_| {});
        let s = at(&s, &p, 2_200, |b| enter(b, 2, CAND_B, BOB, 2_200));
        let s = at(&s, &p, 2_400, |b| cases(b, 0x70, 2_400));
        let s = at(&s, &p, 2_510, |b| {
            for _ in 0..3 {
                assert_eq!(b.pay_improvement_eval_fee_v1(&h(LINE), 2, &PalwEvalSubjectV1::Parent, &bond(CAROL)).unwrap(), fee_per_job);
            }
        });
        let e2 = s.improvement_epoch(&h(LINE), 2).unwrap();
        assert_eq!((e2.state, e2.previous), (PalwEpochStateV1::Evaluating, Some(h(LINE))), "[E20] the check joined at the draw");
        conserved(&s, &h(LINE));
        let bob_before = s.improvement_earnings(&bond(BOB));
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        apply_lineage_rollback_v1(&mut b, &ctx(2_600), &rollback, &bond(OWNER)).unwrap();
        let aborted = b.checkpoint().0;
        let escrow_b = palw_improvement_jobs_per_subject_v1(&policy().eval, true) * fee_per_job;
        assert_eq!(
            aborted.improvement_earnings(&bond(BOB)) - bob_before,
            fees.candidate_bond + escrow_b + (fees.registration_fee - 3 * fee_per_job),
            "B: the bond and its escrow in full, the fee less the spend it funded"
        );
        conserved(&aborted, &h(LINE));

        // (b) [D12, E22] No epoch 2: the first quarter vests on its step; the rollback keeps it.
        let s = at(&promoted, &p, 2_000, |_| {});
        assert!(s.improvement_line(&h(LINE)).unwrap().open_epoch.is_none(), "usage restarted at the promotion");
        assert_eq!(s.improvement_line(&h(LINE)).unwrap().next_due_daa, 2_900, "due at the first vesting step");
        let s = at(&s, &p, 2_900, |_| {});
        assert!(s.improvement_grants(&h(LINE), 1).iter().all(|(_, g)| g.vested == g.amount / 4), "the first quarter vested");
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        apply_lineage_rollback_v1(&mut b, &ctx(2_920), &rollback, &bond(OWNER)).expect("inside the pinned window (3,850)");
        let s = b.checkpoint().0;
        let line = s.improvement_line(&h(LINE)).unwrap();
        assert!(line.is_barred(&bond(ALICE), 2_920 + 8 * 950 - 1) && !line.is_barred(&bond(ALICE), 2_920 + 8 * 950), "the pinned bar");
        for (_, grant) in s.improvement_grants(&h(LINE), 1) {
            assert!(grant.forfeited && grant.vested == grant.amount / 4, "[D12] the vested quarter is kept: {grant:?}");
        }
        conserved(&s, &h(LINE));
    }

    /// **[D2/E21] An abort never refunds more than the balance holds.** An S1 payout may have spent part
    /// of what the fees funded; the owner's rollback is then not refused for want of balance — each fee's
    /// refund is clamped by what the balance holds, and the pool still conserves. Here the whole balance
    /// is spent when the epoch is aborted, one candidate whose fee (10,000) is far above the escrows
    /// (8 jobs a subject at 1 sompi): the unspent parent escrow (5) is all there is to refund.
    #[test]
    fn an_abort_refunds_no_more_than_the_balance_holds() {
        let p = params();
        let mut pol = policy();
        pol.fees.registration_fee = 10_000;
        pol.fees.eval_fee_per_job = 1;
        let s = at(&genesis(), &p, 500, |b| apply_improvement_policy_set_v1(b, &ctx(500), &set(LINE, 1, Some(pol.clone()))).expect("opt in"));
        let mut s = s;
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let s = at(&s, &p, 1_000, |_| {});
        let s = at(&s, &p, 1_200, |b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(CAND_A) },
                h(0x50),
                Vec::new(),
                1_200,
            )
            .unwrap();
        });
        let s = at(&s, &p, 1_400, |b| {
            for i in 0..8u8 {
                b.note_improvement_material_v1(&h(LINE), PalwMaterialKindV1::HardCase, &h(0x60 + i), &bond(CAROL), 1_400).unwrap();
            }
        });
        let s = at(&s, &p, 1_510, |b| {
            for _ in 0..3 {
                assert_eq!(b.pay_improvement_eval_fee_v1(&h(LINE), 1, &PalwEvalSubjectV1::Parent, &bond(CAROL)).unwrap(), 1);
            }
        });
        assert_eq!(s.improvement_epoch(&h(LINE), 1).unwrap().state, PalwEpochStateV1::Evaluating);
        // S1 has spent everything the balance held (the 10,000 less the parent's reserved escrow).
        let mut drained = s.clone();
        let mut pool = drained.improvement_pool(&h(LINE)).unwrap();
        let spent = pool.balance;
        assert!(spent > 5, "the fee is in the balance: {spent}");
        pool.balance = 0;
        pool.paid += spent as u128;
        drained.improvement_pools.insert(h(LINE), pool);
        conserved(&drained, &h(LINE));
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&drained, &p, false, false, false, false, &extras);
        let mut line = drained.improvement_line(&h(LINE)).unwrap().clone();
        decide_epoch_v1(&mut b, &ctx(1_520), &mut line, 1, PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::Aborted })
            .expect("an abort is never refused for want of balance");
        let after = b.checkpoint().0;
        let escrow_a = palw_improvement_jobs_per_subject_v1(&pol.eval, true) * pol.fees.eval_fee_per_job;
        let unspent_parent = palw_improvement_jobs_per_subject_v1(&pol.eval, false) * pol.fees.eval_fee_per_job - 3;
        assert_eq!(
            after.improvement_earnings(&bond(ALICE)),
            pol.fees.candidate_bond + escrow_a + unspent_parent,
            "the bond and the escrow in full, and of the fee only what the balance held (the parent's returned escrow)"
        );
        assert_eq!(after.improvement_pool(&h(LINE)).unwrap().balance, 0, "nothing below zero");
        conserved(&after, &h(LINE));
    }

    /// [D14] An opt-out is filed once; a policy before its `effective_daa` cancels it; past it (the
    /// line not yet dissolved) a policy is refused.
    #[test]
    fn an_opt_out_is_cancelled_before_it_takes_effect_and_filed_once() {
        let p = params();
        let s = opted_in(500);
        let s = at(&s, &p, 700, |b| apply_improvement_policy_set_v1(b, &ctx(700), &set(LINE, 2, None)).unwrap());
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(
            matches!(apply_improvement_policy_set_v1(&mut b, &ctx(800), &set(LINE, 3, None)), Err(PalwStateV2Error::ImprovementPolicyRefused(_))),
            "a second opt-out"
        );
        apply_improvement_policy_set_v1(&mut b, &ctx(800), &set(LINE, 3, Some(policy()))).unwrap();
        let cancelled = b.checkpoint().0;
        assert_eq!(cancelled.improvement_line(&h(LINE)).unwrap().status, PalwImprovementLineStatusV1::Governed);
        // Something still held keeps the line from dissolving past its effective DAA (1,700).
        let mut s = s;
        let mut pool = s.improvement_pool(&h(LINE)).unwrap();
        pool.held += 1;
        pool.held_in += 1;
        s.improvement_pools.insert(h(LINE), pool);
        let s = at(&s, &p, 1_700, |_| {});
        assert_eq!(s.improvement_line(&h(LINE)).unwrap().status, PalwImprovementLineStatusV1::OptingOut { effective_daa: Some(1_700) });
        let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(
            matches!(
                apply_improvement_policy_set_v1(&mut b, &ctx(1_800), &set(LINE, 3, Some(policy()))),
                Err(PalwStateV2Error::ImprovementPolicyRefused(_))
            ),
            "past effective_daa the line opts in again only once dissolved"
        );
    }

    /// **Spec 17 §17.13: `transitions.json` and `pool.json`** — the end-to-end epoch above, traced
    /// block by block (state, head, pool), compared with the files; `IMPROVE_BLESS=1` rewrites them.
    #[test]
    fn the_transition_and_pool_vectors_are_the_folds() {
        use serde_json::json;
        let p = params();
        let mut trace = Vec::new();
        let mut pool_rows = Vec::new();
        let mut record = |s: &PalwChainStateV2, daa: u64, event: &str| {
            let line = s.improvement_line(&h(LINE)).unwrap();
            let epoch = line.open_epoch.or(line.next_epoch.checked_sub(1)).and_then(|e| s.improvement_epoch(&h(LINE), e));
            trace.push(json!({
                "daa": daa,
                "event": event,
                "epoch": epoch.map(|e| e.epoch),
                "state": epoch.map(|e| format!("{:?}", e.state)),
                "outcome": epoch.and_then(|e| e.outcome).map(|o| format!("{o:?}")),
                "head": line.head.to_string(),
                "next_due_daa": line.next_due_daa,
            }));
            let pool = s.improvement_pool(&h(LINE)).unwrap();
            pool_rows.push(json!({
                "daa": daa,
                "balance": pool.balance, "held": pool.held, "unvested": pool.unvested,
                "fees_in": pool.fees_in.to_string(), "held_in": pool.held_in.to_string(), "deposited": pool.deposited.to_string(),
                "paid": pool.paid.to_string(), "refunded": pool.refunded.to_string(), "forfeited_in": pool.forfeited_in.to_string(),
            }));
        };
        let mut s = opted_in(500);
        record(&s, 500, "opt in (policy: n 8, n_min 4, k_max 2, grid 1000)");
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        let s = at(&s, &p, 1_000, |b| {
            let deposit = PalwImprovementPoolFundingV1 { line_id: h(LINE), amount: 50_000_000_000, sink_index: 1 };
            apply_improvement_pool_funded_v1(b, &ctx(1_000), &deposit).unwrap();
        });
        record(&s, 1_000, "grid boundary: usage 5 ≥ 2 opens epoch 1; a sponsor deposits 500 MSK");
        let s = at(&s, &p, 1_200, |b| {
            for (class, who) in [(CAND_A, ALICE), (CAND_B, BOB)] {
                b.admit_improvement_candidate_v1(
                    &h(LINE),
                    1,
                    &h(class),
                    &bond(who),
                    crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(class) },
                    h(0x50),
                    Vec::new(),
                    1_200,
                )
                .unwrap();
            }
        });
        record(&s, 1_200, "t_fix: dataset_root fixed; candidates A and B enter");
        let s = at(&s, &p, 1_400, |b| {
            for i in 0..8u8 {
                b.note_improvement_material_v1(
                    &h(LINE),
                    PalwMaterialKindV1::HardCase,
                    &h(0x60 + i),
                    &bond(if i % 2 == 0 { CAROL } else { OWNER }),
                    1_400,
                )
                .unwrap();
            }
        });
        record(&s, 1_400, "t_close: frozen; eight hold-out cases");
        let s = at(&s, &p, 1_510, |b| {
            for item in 0..8u32 {
                for (subject, value) in [
                    (PalwEvalSubjectV1::Parent, 0),
                    (PalwEvalSubjectV1::Candidate(h(CAND_A)), 1),
                    (PalwEvalSubjectV1::Candidate(h(CAND_B)), 0),
                ] {
                    let score = PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value };
                    b.record_improvement_score_v1(&h(LINE), 1, item, subject, score).unwrap();
                    b.pay_improvement_eval_fee_v1(&h(LINE), 1, &subject, &bond(CAROL)).unwrap();
                }
            }
        });
        record(&s, 1_510, "t_draw + d: drawn; every score recorded and every job paid");
        let s = at(&s, &p, 1_800, |_| {});
        record(&s, 1_800, "t_eval: closing");
        let s = at(&s, &p, 1_950, |_| {});
        record(&s, 1_950, "t_score: A promoted 8–0; B refunded; grants made");
        let s = at(&s, &p, 1_950 + 950, |_| {});
        record(&s, 2_900, "one vesting unit later (L_e = 950): a quarter vested");
        let s = at(&s, &p, 1_950 + 4 * 950, |_| {});
        record(&s, 5_750, "four units: every grant vested");
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/improve-v1");
        let bless = std::env::var("IMPROVE_BLESS").is_ok_and(|v| v == "1");
        for (name, rows) in [("transitions", trace), ("pool", pool_rows)] {
            let value = json!({ "scenario": "palw_improve_fold_v1 tests: the end-to-end epoch (spec 17 §17.13)", "rows": rows });
            let bytes = serde_json::to_string_pretty(&value).unwrap() + "\n";
            let path = dir.join(format!("{name}.json"));
            if bless {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &bytes).unwrap();
            } else {
                let on_disk =
                    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (IMPROVE_BLESS=1 writes it)", path.display()));
                assert!(on_disk == bytes, "{} differs from the fold's (IMPROVE_BLESS=1 rewrites it)", path.display());
            }
        }
        conserved(&s, &h(LINE));
        assert_eq!(s.improvement_pool(&h(LINE)).unwrap().unvested, 0, "everything vested");
    }

    /// **Every step replays and reverts by its delta, indices included** (the reviewer's invariant):
    /// the due index, the heads index, the open-epoch count, the live results and the retiring index
    /// all follow `apply_delta_v2` / `revert_delta_v2` through an epoch's whole life and its retirement.
    #[test]
    fn every_step_replays_and_reverts_by_its_delta() {
        let p = params();
        let step = |parent: &PalwChainStateV2, daa: u64, f: &dyn Fn(&mut TransitionBuilder<'_>)| -> PalwChainStateV2 {
            let extras = PalwTransitionExtrasV1::default();
            let mut builder = TransitionBuilder::new(parent, &p, false, false, false, false, &extras);
            advance_improvement_v1(&mut builder, &ctx(daa)).unwrap();
            f(&mut builder);
            let child = builder.checkpoint().0;
            let delta = PalwStateDeltaV2 { point: ctx(daa), entries: builder.entries.clone() };
            assert_eq!(apply_delta_v2(parent, &delta, &p).unwrap(), child, "DAA {daa}: the delta replays");
            assert_eq!(revert_delta_v2(&child, &delta, &p).unwrap(), *parent, "DAA {daa}: the delta reverts");
            // The incremental indices are exactly what a restarted node rebuilds from the rows.
            let mut rebuilt = child.clone();
            rebuild_improvement_indices_v1(&mut rebuilt);
            assert_eq!(rebuilt, child, "DAA {daa}: the incremental indices are the rebuilt ones");
            child
        };
        let mut s = opted_in(500);
        s.improvement_usage.insert(h(LINE), PalwImprovementUsageV1 { usage: 5, since_daa: 500 });
        rebuild_improvement_indices_v1(&mut s);
        let s = step(&s, 1_000, &|_| {});
        assert_eq!(s.improvement_open_epoch_count(), 1);
        assert!(s.improvement_live_results() > 0, "the epoch reserved its results");
        let s = step(&s, 1_200, &|b| {
            b.admit_improvement_candidate_v1(
                &h(LINE),
                1,
                &h(CAND_A),
                &bond(ALICE),
                crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(1) },
                h(0x50),
                Vec::new(),
                1_200,
            )
            .unwrap();
        });
        let s = step(&s, 1_400, &|b| {
            for i in 0..8u8 {
                b.note_improvement_material_v1(
                    &h(LINE),
                    PalwMaterialKindV1::HardCase,
                    &h(0x60 + i),
                    &bond(if i % 2 == 0 { CAROL } else { OWNER }),
                    1_400,
                )
                .unwrap();
            }
        });
        let s = step(&s, 1_510, &|b| {
            for item in 0..8u32 {
                for (subject, value) in [(PalwEvalSubjectV1::Parent, 0), (PalwEvalSubjectV1::Candidate(h(CAND_A)), 1)] {
                    b.record_improvement_score_v1(
                        &h(LINE),
                        1,
                        item,
                        subject,
                        PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value },
                    )
                    .unwrap();
                }
            }
        });
        let s = step(&s, 1_950, &|_| {});
        assert_eq!(s.improvement_open_epoch_count(), 0);
        assert_eq!(s.improvement_retiring.len(), 1, "the decided epoch's details wait for the sweep");
        let s = step(&s, 1_951, &|_| {});
        assert!(s.improvement_retiring.is_empty() && s.improvement_live_results() == 0, "retired, and the reservation freed");
        assert_eq!(s.improvement_lines_headed_by(&h(CAND_A)), vec![h(LINE)], "the heads index follows the promotion");
    }

    #[test]
    fn a_deposit_credits_a_governed_pool_and_nothing_else() {
        let p = params();
        let s = opted_in(500);
        let s = at(&s, &p, 600, |b| {
            let deposit = PalwImprovementPoolFundingV1 { line_id: h(LINE), amount: 5_000, sink_index: 1 };
            apply_improvement_pool_funded_v1(b, &ctx(600), &deposit).unwrap();
            let stranger = PalwImprovementPoolFundingV1 { line_id: h(CAND_A), amount: 5_000, sink_index: 1 };
            assert!(apply_improvement_pool_funded_v1(b, &ctx(600), &stranger).is_err(), "not a governed line");
            let nothing = PalwImprovementPoolFundingV1 { line_id: h(LINE), amount: 0, sink_index: 1 };
            assert!(apply_improvement_pool_funded_v1(b, &ctx(600), &nothing).is_err());
        });
        let pool = s.improvement_pool(&h(LINE)).unwrap();
        assert_eq!((pool.balance, pool.deposited), (5_000, 5_000));
        conserved(&s, &h(LINE));
    }

    #[test]
    fn the_earnings_flush_is_bounded() {
        let p = params();
        let mut s = opted_in(500);
        for who in 0..5u8 {
            s.improvement_earnings.insert(bond(0x90 + who), 10);
        }
        let s = at(&s, &p, 600, |_| {});
        assert_eq!(s.improvement_earnings.len(), 3, "two paid out per block");
        assert_eq!(s.pending_payouts.len(), 0, "a bond that does not exist is paid nothing (burned)");
    }
}
